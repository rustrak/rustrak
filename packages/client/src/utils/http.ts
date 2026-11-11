import type { ClientConfig } from '../config.js';
import {
  FIELD_ERROR_CODES,
  type FieldError,
  type FieldErrorCode,
  NETWORK_ERROR_MESSAGE,
  type RustrakError,
  SERVER_ERROR_MESSAGE,
  TIMEOUT_ERROR_MESSAGE,
  TRANSIENT_STATUSES,
} from '../errors.js';

/**
 * Internal carrier for a transport failure travelling out of `fetch`.
 *
 * `fetch` signals "no response" by rejecting with a `TypeError`, which is also
 * what a genuine bug throws. {@link createFetch} wraps the rejections it
 * recognises in this class so the `Result` boundary in `client.ts` can tell
 * the two apart: this becomes an `Err`, anything else keeps propagating.
 *
 * Never exported from `src/index.ts`.
 */
export class RustrakTransportFailure extends Error {
  public readonly rustrakError: RustrakError;

  constructor(error: RustrakError) {
    super(error.message);
    this.name = 'RustrakTransportFailure';
    this.rustrakError = error;
  }
}

/**
 * Read the human-readable message out of an error body.
 *
 * Two shapes are live: every `AppError` sends `{error: {type, message}}` while
 * the ingest rate limiter sends a flat `{error: "..."}`. Reading only the flat
 * one turned every structured error into "[object Object]". See #204.
 */
function readErrorMessage(body: unknown, status: number): string {
  const fallback = `HTTP ${status} error`;

  // `typeof body === 'object'` on purpose: a non-JSON body arrives as a plain
  // string, and property access on a string primitive yields undefined, so a
  // bare `if (body)` fell through to the generic message only by accident.
  // Make the fallback deliberate.
  if (body && typeof body === 'object') {
    const { error, message } = body as {
      error?: string | { type?: string; message?: string };
      message?: string;
    };
    const nested = typeof error === 'object' ? error?.message : error;
    return nested || message || fallback;
  }

  return fallback;
}

const KNOWN_FIELD_ERROR_CODES = new Set<string>(FIELD_ERROR_CODES);

function isFieldErrorCode(value: unknown): value is FieldErrorCode {
  return typeof value === 'string' && KNOWN_FIELD_ERROR_CODES.has(value);
}

/**
 * Codes already reported by {@link warnUnknownFieldErrorCode}.
 *
 * Module-scoped so a list page rendering fifty rows against a newer server
 * logs once per unknown code rather than once per row. Version skew does not
 * change within a process, so once is the whole signal.
 */
const warnedFieldErrorCodes = new Set<string>();

/**
 * Make an unrecognised `code` visible without admitting it into the union.
 *
 * Dropping the entry is correct (see {@link readFieldErrors}), but a silent
 * drop is indistinguishable from "the server named no field": a client one
 * release behind a server that added a code loses every inline form error
 * site-wide with nothing anywhere to explain it.
 *
 * `console.warn` is the least intrusive mechanism that a support engineer can
 * actually act on. It needs no change to the public type, so no consumer has
 * to opt in to see it; it lands in the browser devtools console in a client
 * component and on stdout of the Node process (and therefore in whatever log
 * aggregation already exists) in a Server Component. The alternatives were
 * worse: throwing turns a cosmetic degradation into an outage, admitting the
 * code breaks the closed union this drop exists to protect, and a new
 * `fieldsDropped` flag on the union is invisible until somebody thinks to read
 * it, which is the same silence in a new place.
 */
function warnUnknownFieldErrorCode(code: string, field: string): void {
  if (warnedFieldErrorCodes.has(code)) return;
  warnedFieldErrorCodes.add(code);

  console.warn(
    `[@rustrak/client] Dropped a field error with an unrecognised code ` +
      `"${code}" (field "${field}"). This client build does not know that ` +
      `code, so the affected inputs degrade to a form-level error. The ` +
      `server is most likely newer than @rustrak/client; upgrade the client ` +
      `to match the server version.`,
  );
}

/**
 * Read `error.incident_id` out of a 5xx `AppError` body.
 *
 * The one thing a 500 is willing to say about itself. `error_response` replaced
 * its `Display` with a fixed string so a `sqlx::Error`'s constraint and column
 * names stay off the wire (gh-233), and logged the detail under this id; a user
 * quoting it is what lets an operator find that line.
 *
 * Returns `undefined`, never `null` or `''`, for the three bodies that carry no
 * id: a proxy-generated 502/503, a server older than gh-233, and a 500 whose
 * body is not JSON at all. The caller omits the key entirely in that case, so
 * `'incidentId' in error` stays an honest test.
 */
function readIncidentId(body: unknown): string | undefined {
  const raw =
    body && typeof body === 'object'
      ? (body as { error?: { incident_id?: unknown } }).error?.incident_id
      : undefined;

  return typeof raw === 'string' && raw.length > 0 ? raw : undefined;
}

/**
 * Read `error.fields` out of an `AppError` body.
 *
 * Returns `undefined` rather than `[]` when there is nothing usable, so the
 * key stays absent on the `RustrakError` and a consumer can test for it.
 *
 * Entries are validated one by one and a malformed or unrecognised one is
 * dropped, never coerced. `FieldErrorCode` is a closed union: admitting a code
 * this build has never heard of would hand a consumer a value its own
 * exhaustive `switch` cannot see. A newer server adding a code therefore
 * degrades to a form-level error here, which is the same place an older client
 * already lands, and is announced once via
 * {@link warnUnknownFieldErrorCode} so the skew is not silent.
 *
 * `message` survives only alongside `code: 'custom'`. The server enforces the
 * same rule at construction, but the docs on both sides tell consumers to
 * select translated copy from `(field, code)` for every other code, so a
 * `message` that reaches them there is untranslatable English a careless
 * consumer would render. Dropping it here makes the rule true of the value, not
 * only of its producer.
 */
function readFieldErrors(body: unknown): FieldError[] | undefined {
  const raw =
    body && typeof body === 'object'
      ? (body as { error?: { fields?: unknown } }).error?.fields
      : undefined;

  if (!Array.isArray(raw)) {
    return undefined;
  }

  const fields = raw.flatMap((entry) => readFieldError(entry) ?? []);
  return fields.length > 0 ? fields : undefined;
}

/** One entry of `error.fields`, or `undefined` when it is unusable. */
function readFieldError(entry: unknown): FieldError | undefined {
  if (typeof entry !== 'object' || entry === null) return undefined;

  const { field, code, message } = entry as Record<string, unknown>;
  if (typeof field !== 'string') return undefined;
  if (!isFieldErrorCode(code)) {
    if (typeof code === 'string') warnUnknownFieldErrorCode(code, field);
    return undefined;
  }

  return code === 'custom' && typeof message === 'string'
    ? { field, code, message }
    : { field, code };
}

/**
 * `Retry-After` in seconds, when the server sent a parseable one.
 *
 * RFC 9110 §10.2.3 allows two forms and a proxy in front of the Rust process
 * may send either. `Number.parseInt` read only the first, and read it too
 * loosely: it dropped the HTTP-date form entirely as `NaN`, and it returned
 * `-5` from `-5` and `3` from `3abc`, both of which a consumer would pass
 * straight to `setTimeout`.
 *
 * A date already in the past yields `0`, meaning "retry now", which is the
 * literal reading of the header.
 */
function readRetryAfter(response: Response): number | undefined {
  const header = response.headers.get('Retry-After')?.trim();
  if (!header) {
    return undefined;
  }

  // delta-seconds: digits only. No sign, no trailing junk.
  if (/^\d+$/.test(header)) {
    return Number.parseInt(header, 10);
  }

  // A signed number is neither a valid delta-seconds nor an HTTP-date, and it
  // must not reach `Date.parse`: V8 reads `-5` as a year and would turn a
  // nonsense header into "retry now".
  if (/^[+-]\d+$/.test(header)) {
    return undefined;
  }

  const date = Date.parse(header);
  if (Number.isNaN(date)) {
    return undefined;
  }

  return Math.max(0, Math.ceil((date - Date.now()) / 1000));
}

/**
 * Attach `fields` to an error, or leave the key off entirely.
 *
 * The key is omitted rather than set to `undefined` so `'fields' in error`
 * stays an honest test and the object round-trips through `structuredClone`
 * with the same shape it was built with.
 */
function annotate<E extends RustrakError & { fields?: readonly FieldError[] }>(
  base: E,
  fields: readonly FieldError[] | undefined,
): E {
  return fields === undefined ? base : { ...base, fields };
}

/**
 * Map an HTTP status onto a `RustrakError` kind.
 *
 * Total by construction: every 5xx redacts, every other unanticipated status
 * lands on `client_error`, so no status is unrepresentable.
 */
export function transformHttpError(
  response: Response,
  body: unknown,
): RustrakError {
  const status = response.status;

  // 5xx never carries a server-supplied message. The server redacts its own
  // `Display` now (gh-233), but this stays: a 502 from a reverse proxy and a
  // server older than that fix both still send prose, and every consumer of
  // this client renders `error.message` somewhere. Discard it here, once,
  // rather than trusting 128 call sites to remember.
  //
  // `incidentId` is the exception, and the reason the redaction is no longer
  // lossy: it names the log line carrying what the body dropped, and is
  // omitted entirely when the body has none.
  if (status >= 500) {
    const base = {
      kind: 'server_error',
      status,
      message: SERVER_ERROR_MESSAGE,
    } as const;
    const incidentId = readIncidentId(body);

    return incidentId === undefined ? base : { ...base, incidentId };
  }

  const message = readErrorMessage(body, status);

  // Read once, for every non-5xx status. `AppError::with_field` works on all
  // eight Rust variants, so restricting this to 400 and 409 meant an
  // annotation added to, say, the `Forbidden` in `routes/team.rs` would be
  // serialised, documented in `openapi.json`, and invisible to every consumer
  // with no test failing anywhere. It costs nothing when the key is absent:
  // `readFieldErrors` returns `undefined` without allocating.
  const fields = readFieldErrors(body);

  switch (status) {
    case 400:
      return annotate({ kind: 'validation', status, message }, fields);
    case 401:
      return annotate({ kind: 'unauthenticated', status, message }, fields);
    case 403:
      return annotate({ kind: 'forbidden', status, message }, fields);
    case 404:
      return annotate({ kind: 'not_found', status, message }, fields);
    case 409:
      return annotate({ kind: 'conflict', status, message }, fields);
    case 410:
      return annotate({ kind: 'gone', status, message }, fields);
    case 413:
      return annotate({ kind: 'payload_too_large', status, message }, fields);
    case 429: {
      const retryAfter = readRetryAfter(response);
      return annotate(
        retryAfter === undefined
          ? { kind: 'rate_limited', status, message }
          : { kind: 'rate_limited', status, message, retryAfter },
        fields,
      );
    }
    default:
      return annotate({ kind: 'client_error', status, message }, fields);
  }
}

/** Statuses a `Response` may not be constructed with a body for. */
const NULL_BODY = new Set([101, 103, 204, 205, 304]);

/**
 * Only methods that are safe to send twice. A `POST` that timed out at the
 * proxy may still have created the row, and a second one creates another.
 */
const RETRY_METHODS = new Set(['GET', 'HEAD', 'PUT', 'DELETE']);

/**
 * The `fetch` every request goes through: timeout, retries, and the line
 * between a transport failure and a bug.
 *
 * The body is read here, inside the same `try` as the request, so a
 * connection that dies halfway through a body is a transport failure like
 * one that never connected, not an error the generated client half-parses.
 *
 * Retries wait 300ms, then 600ms, and so on. A caller's own `AbortSignal`
 * aborting is not a transport failure: it rethrows as the `AbortError` the
 * caller asked for.
 */
export function createFetch(config: ClientConfig): typeof fetch {
  const timeout = config.timeout ?? 30_000;
  const maxRetries = config.maxRetries ?? 2;

  return async (input, init) => {
    const request = new Request(input, init);
    const retries = RETRY_METHODS.has(request.method) ? maxRetries : 0;

    for (let n = 0; ; n++) {
      const outcome = await attempt(request, timeout);
      // `isRetryable`'s rule, less the two failures whose wait only the
      // caller can pick: a 429 names its own in `Retry-After`, and a request
      // that timed out may still be running on the server.
      const retryable =
        outcome instanceof Response
          ? outcome.status !== 429 && TRANSIENT_STATUSES.has(outcome.status)
          : outcome.reason === 'unreachable';
      if (n < retries && retryable) {
        await sleep(n);
        continue;
      }
      if (outcome instanceof Response) return outcome;
      throw new RustrakTransportFailure(outcome);
    }
  };
}

/** One try: the response with its body read, or the failure it ended in. */
async function attempt(
  request: Request,
  timeout: number,
): Promise<Response | Extract<RustrakError, { kind: 'network' }>> {
  try {
    const response = await fetch(request.clone(), {
      signal: AbortSignal.any([request.signal, AbortSignal.timeout(timeout)]),
    });
    const body = await response.arrayBuffer();
    // The generated client returns `{}` unvalidated for `Content-Length: 0`.
    // Without it an empty 200 is parsed, and validated, like any other.
    const headers = new Headers(response.headers);
    headers.delete('Content-Length');
    return new Response(NULL_BODY.has(response.status) ? null : body, {
      status: response.status,
      statusText: response.statusText,
      headers,
    });
  } catch (error) {
    if (request.signal.aborted) throw error;
    // No `cause`, and never `error.message`: both embed the resolved host and
    // port, which must not ride along into a serialized payload or a UI.
    return error instanceof DOMException && error.name === 'TimeoutError'
      ? { kind: 'network', message: TIMEOUT_ERROR_MESSAGE, reason: 'timeout' }
      : {
          kind: 'network',
          message: NETWORK_ERROR_MESSAGE,
          reason: 'unreachable',
        };
  }
}

function sleep(attempt: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 300 * 2 ** attempt));
}
