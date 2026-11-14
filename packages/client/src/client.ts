// The core class: what Zod Mini throws, and what classic `ZodError` extends.
import { $ZodError as ZodError } from 'zod/v4/core';
import type { ClientConfig } from './config.js';
import type { RustrakError } from './errors.js';
import { BINARY } from './generated/binary.gen.js';
import { createClient, createConfig } from './generated/client/index.js';
import { Rustrak } from './generated/sdk.gen.js';
import { Err, Ok, type Result } from './result.js';
import {
  createFetch,
  RustrakTransportFailure,
  transformHttpError,
} from './utils/http.js';

/**
 * Operations whose 2xx is a file, read as a `Blob`. The spec leaves the body
 * untyped (zod has no binary type), so the type comes from this list.
 */
type Binary = (typeof BINARY)[number];
const BINARY_OPERATIONS: ReadonlySet<string> = new Set(BINARY);

/** What a caller may pass: the request's own parts, never client plumbing. */
type RequestKeys = 'path' | 'query' | 'body' | 'headers' | 'signal';

type Call<F, Bytes extends boolean> = F extends (options: infer O) => infer R
  ? (
      ...args: {} extends Pick<O, Extract<keyof O, RequestKeys>>
        ? [options?: Pick<O, Extract<keyof O, RequestKeys>>]
        : [options: Pick<O, Extract<keyof O, RequestKeys>>]
    ) => Promise<
      Result<Bytes extends true ? Blob : Data<Awaited<R>>, RustrakError>
    >
  : never;

/** The success half of the generated `{ data, error }` union. */
type Data<R> =
  Extract<R, { error: undefined }> extends { data: infer D } ? D : never;

type Resource<T, Name extends string> = {
  readonly [M in keyof T]: Call<
    T[M],
    `${Name}.${M & string}` extends Binary ? true : false
  >;
};

/**
 * Every operation in `openapi.json`, grouped by tag:
 * `client.issues.list({ path: { project_id } })`.
 *
 * Generated in `src/generated/`; this file only changes what a call returns.
 */
export type RustrakClient = {
  readonly [R in keyof Rustrak]: Resource<Rustrak[R], R & string>;
};

/**
 * Main Rustrak API client. Every method returns a `Result`; nothing throws for
 * an expected failure.
 *
 * @example
 * ```typescript
 * const client = new RustrakClient({
 *   baseUrl: 'http://localhost:8080',
 *   token: 'your-api-token',
 * });
 *
 * const issues = await client.issues.list({ path: { project_id: 1 } });
 * if (issues.success) console.log(issues.data.items);
 * ```
 */
export const RustrakClient = class {
  constructor(config: ClientConfig) {
    // biome-ignore lint/correctness/noConstructorReturn: the instance is the generated SDK with every method wrapped
    return Object.setPrototypeOf(createApi(config), new.target.prototype);
  }
} as unknown as new (
  config: ClientConfig,
) => RustrakClient;

function createApi(config: ClientConfig): RustrakClient {
  const sdk = new Rustrak({
    client: createClient(
      createConfig({
        baseUrl: config.baseUrl,
        credentials: 'include',
        fetch: createFetch(config),
        // Every 2xx in the spec is JSON or empty, except the files in
        // `BINARY`. `auto` would hand back an HTML page or a stream as a
        // success without validating it.
        parseAs: 'json',
        headers: {
          // Without it a content-negotiating hop can answer with HTML.
          Accept: 'application/json',
          ...config.headers,
          ...(config.token && { Authorization: `Bearer ${config.token}` }),
        },
      }),
    ),
  });

  const api: Record<string, Record<string, unknown>> = {};
  for (const name of Object.getOwnPropertyNames(Rustrak.prototype)) {
    if (name === 'constructor') continue;
    const resource = sdk[name as keyof Rustrak] as unknown as Record<
      string,
      (options: object) => Promise<Outcome>
    >;
    const methods: Record<string, unknown> = {};
    for (const method of Object.getOwnPropertyNames(
      Object.getPrototypeOf(resource),
    )) {
      if (method === 'constructor') continue;
      methods[method] = async (options: { headers?: unknown } = {}) =>
        toResult(
          await resource[method]!({
            ...options,
            // A JSON write spreads these into an object, which keeps nothing
            // of a `Headers` or an array of pairs.
            headers:
              options.headers instanceof Headers ||
              Array.isArray(options.headers)
                ? Object.fromEntries(new Headers(options.headers))
                : options.headers,
            ...(BINARY_OPERATIONS.has(`${name}.${method}`) && {
              parseAs: 'blob',
            }),
            throwOnError: false,
          }),
        );
    }
    api[name] = methods;
  }
  return api as unknown as RustrakClient;
}

type Outcome = {
  data?: unknown;
  error?: unknown;
  response?: Response;
};

/**
 * The boundary: the generated client's `{ data, error, response }` becomes a
 * `Result`.
 *
 * Only failures this file recognises become an `Err`. Anything else, a
 * `TypeError` from a genuine bug, is rethrown so it crashes where it can be
 * debugged instead of being laundered into `{ success: false }`.
 */
function toResult({
  data,
  error,
  response,
}: Outcome): Result<unknown, RustrakError> {
  if (error === undefined) return Ok(data);
  if (error instanceof RustrakTransportFailure) return Err(error.rustrakError);
  if (response && !response.ok) {
    // `error` is the parsed body, or its text when it is not JSON.
    return Err(transformHttpError(response, error));
  }
  if (!response && error instanceof ZodError) {
    return Err({
      kind: 'invalid_request',
      message: 'The request input did not match the expected schema.',
    });
  }
  if (response && error instanceof SyntaxError) {
    return Err({
      kind: 'invalid_response',
      message: 'The server returned a body that is not valid JSON.',
    });
  }
  if (response && error instanceof ZodError) {
    // No Zod issues: they embed the offending response data.
    return Err({
      kind: 'invalid_response',
      message: 'The server response did not match the expected schema.',
    });
  }
  throw error;
}
