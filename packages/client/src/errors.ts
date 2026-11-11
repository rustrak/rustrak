/**
 * The one closed error union `@rustrak/client` reports.
 *
 * It mirrors `apps/server/src/error.rs` variant by variant, plus the three
 * failures that never reach the server (`invalid_request`, `network`,
 * `invalid_response`) and a catch-all so an unanticipated status is still
 * representable rather than unrepresentable.
 *
 * Every member is a plain object: no class, no prototype chain, no `Error`
 * instance. That is what lets a failed `Result` survive `structuredClone` and
 * therefore React's server/client boundary.
 *
 * `status` is `number`, never a literal union. Pinning literals would make a
 * proxy-generated 502 or a future status a type error at the call site rather
 * than a value the consumer can log.
 *
 * `fields` appears on every member that can carry an `AppError` body, because
 * `AppError::with_field` works on all eight Rust variants: a `Forbidden` or a
 * `NotFound` may name an input just as a `Conflict` does. Only the members
 * that never carry a server body at all (`server_error`, which discards it,
 * plus `invalid_request`, `network` and `invalid_response`, which never had
 * one) omit it. See {@link FieldError}.
 */
export type RustrakError =
  /**
   * 400. `AppError::Validation`: the server rejected the request.
   *
   * `fields` is present only when the server named the offending inputs; a
   * consumer that ignores it behaves exactly as it did before.
   */
  | {
      readonly kind: 'validation';
      readonly status: number;
      readonly message: string;
      readonly fields?: readonly FieldError[];
    }
  /** 401. `AppError::Unauthorized`: no session, or the session expired. */
  | {
      readonly kind: 'unauthenticated';
      readonly status: number;
      readonly message: string;
      readonly fields?: readonly FieldError[];
    }
  /** 403. `AppError::Forbidden`: authenticated, but not allowed. */
  | {
      readonly kind: 'forbidden';
      readonly status: number;
      readonly message: string;
      readonly fields?: readonly FieldError[];
    }
  /** 404. `AppError::NotFound`. */
  | {
      readonly kind: 'not_found';
      readonly status: number;
      readonly message: string;
      readonly fields?: readonly FieldError[];
    }
  /**
   * 409. `AppError::Conflict`.
   *
   * Where `fields` earns its keep: uniqueness is the thing only the server can
   * know, and a taken name or slug is a 409, not a 400.
   */
  | {
      readonly kind: 'conflict';
      readonly status: number;
      readonly message: string;
      readonly fields?: readonly FieldError[];
    }
  /** 410. Reserved for endpoints that retire a resource rather than delete it. */
  | {
      readonly kind: 'gone';
      readonly status: number;
      readonly message: string;
      readonly fields?: readonly FieldError[];
    }
  /** 413. `AppError::PayloadTooLarge`: envelope ingestion only. */
  | {
      readonly kind: 'payload_too_large';
      readonly status: number;
      readonly message: string;
      readonly fields?: readonly FieldError[];
    }
  /** 429. The ingest rate limiter (`routes/ingest.rs`), the one flat body. */
  | {
      readonly kind: 'rate_limited';
      readonly status: number;
      readonly message: string;
      /** Seconds to wait, from the `Retry-After` header, when the server sent one. */
      readonly retryAfter?: number;
      readonly fields?: readonly FieldError[];
    }
  /**
   * Any status below 500 that no other member claims. The catch-all that keeps
   * the union closed but total.
   *
   * Not only 4xx: a 3xx that reached the error path (a redirect `fetch` did
   * not follow), a 408, an unenumerated 4xx, and an opaque `status: 0` all land
   * here. `status` is the field to read; `kind` only says "no dedicated
   * handling exists for this one".
   */
  | {
      readonly kind: 'client_error';
      readonly status: number;
      readonly message: string;
      readonly fields?: readonly FieldError[];
    }
  /**
   * The caller's own input failed a pre-flight check. Never reached the
   * network, so it has no status.
   */
  | { readonly kind: 'invalid_request'; readonly message: string }
  /**
   * Any 5xx. `AppError::Database` and `AppError::Internal` both land here, and
   * so does anything a reverse proxy generates.
   *
   * `message` is always {@link SERVER_ERROR_MESSAGE}: the server's body is
   * discarded at construction, inside the client, so no consumer can leak a
   * connection string or a filesystem path into a UI. `incidentId` names the
   * server log line that holds what was discarded, when the body carries one.
   */
  | {
      readonly kind: 'server_error';
      readonly status: number;
      readonly message: string;
      readonly incidentId?: string;
    }
  /**
   * The request never got an HTTP response: DNS, connection refused, TLS,
   * timeout, abort. Also a connection that died while the body was still being
   * read, which is a transport failure even though a status line arrived.
   *
   * `message` is always {@link NETWORK_ERROR_MESSAGE} or
   * {@link TIMEOUT_ERROR_MESSAGE}, never the underlying error's own: fetch
   * builds that message by interpolating the request method and URL
   * (`Request failed due to a network error: GET http://rustrak.internal:8080/api/projects`),
   * so echoing it prints a self-hosted deployment's internal host and port into
   * any UI that renders `error.message`.
   *
   * Carries no `cause` for the same reason. `reason` is the machine-readable
   * discriminator a caller needs instead, derived from the error's `name` and
   * never from its message.
   */
  | {
      readonly kind: 'network';
      readonly message: string;
      /** `timeout` when the client gave up waiting; `unreachable` otherwise. */
      readonly reason: 'timeout' | 'unreachable';
    }
  /**
   * A 2xx arrived but the body is not what the schema promises: not JSON, or
   * JSON of the wrong shape.
   *
   * Carries no Zod issues: they embed the offending response data.
   */
  | { readonly kind: 'invalid_response'; readonly message: string };

export type RustrakErrorKind = RustrakError['kind'];

/**
 * Every `code` a {@link FieldError} may carry, mirroring the `FieldErrorCode`
 * enum in `apps/server/src/error.rs`.
 *
 * Deliberately tiny and resource-agnostic. `tests/unit/app-error-contract.test.ts`
 * parses that Rust enum and fails if this list stops matching it, so a variant
 * added on the server cannot ship without a matching entry here.
 *
 * Exported as a value, not only a type, so a consumer can exhaustively switch
 * or validate against it at runtime.
 */
export const FIELD_ERROR_CODES = [
  'required',
  'invalid',
  'already_exists',
  'too_short',
  'too_long',
  'incorrect',
  'custom',
] as const;

export type FieldErrorCode = (typeof FIELD_ERROR_CODES)[number];

/**
 * One input the server rejected, named as data.
 *
 * `field` is a **dot path into the request body** (`slug`,
 * `credentials.webhook_url`), which is exactly what a form library's
 * `setError` takes.
 *
 * Select the copy shown to the user from `(field, code)` and never from
 * `message`: that is what makes it translatable. `message` is populated only
 * for `code: 'custom'`, where the code set genuinely cannot express the
 * reason, and is then rendered verbatim. The client strips a `message` that
 * arrives on any other code, so the rule holds even against a server that
 * breaks it.
 *
 * The same `(field, code)` pair can mean two things at two statuses:
 * `(role, 'invalid')` is "not a role at all" on a 400 and "a real role, but
 * not acceptable right now" on a 409. Branch on `kind` as well when the copy
 * has to differ.
 *
 * A `field` naming an input the form does not have must fall back to a
 * form-level error. Passing an unknown path to `setError` registers a
 * phantom field that nothing can ever clear.
 */
export type FieldError = {
  readonly field: string;
  readonly code: FieldErrorCode;
  readonly message?: string;
};

/**
 * The fixed message every `server_error` carries. Exported so a consumer (or a
 * test) can assert that nothing server-supplied survived the 5xx redaction.
 */
export const SERVER_ERROR_MESSAGE = 'The server failed to handle the request.';

/**
 * The fixed message a `network` error carries when the request never reached
 * the server. Fixed for the same reason as {@link SERVER_ERROR_MESSAGE}: the
 * underlying message interpolates the request URL, which is the deployment's
 * internal host and port. Read `reason` to branch, not `message`.
 */
export const NETWORK_ERROR_MESSAGE = 'The request could not reach the server.';

/** The fixed message a `network` error carries when the client timed out. */
export const TIMEOUT_ERROR_MESSAGE = 'The request timed out.';

/**
 * Statuses where the same request may well succeed a moment later. The one
 * rule behind {@link isRetryable} and the transport's automatic retries.
 */
export const TRANSIENT_STATUSES: ReadonlySet<number> = new Set([
  408, 429, 500, 502, 503, 504,
]);

/**
 * Whether retrying the same call could plausibly succeed: a request that never
 * got a response, or a transient status. A `rate_limited` error says how long
 * to wait in `retryAfter`. Everything else is deterministic: the same request
 * will fail the same way.
 */
export function isRetryable(error: RustrakError): boolean {
  return (
    error.kind === 'network' ||
    ('status' in error && TRANSIENT_STATUSES.has(error.status))
  );
}
