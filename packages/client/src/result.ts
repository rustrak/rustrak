/**
 * `Result` is how `@rustrak/client` reports an expected failure.
 *
 * The shape deliberately mirrors Zod's `safeParse`: discriminant `success`,
 * payload `data`. Everything inside is a plain object with `Object.prototype`,
 * no class instances and no methods, because React's Flight serializer refuses
 * to send anything else across the server/client boundary.
 */
export type Result<T, E> =
  | { readonly success: true; readonly data: T }
  | { readonly success: false; readonly error: E };

export function Ok<T>(data: T): Result<T, never> {
  return { success: true, data };
}

export function Err<E>(error: E): Result<never, E> {
  return { success: false, error };
}

/**
 * Get the value, throwing if the `Result` is a failure.
 *
 * This is the caller explicitly opting back into exceptions. Nothing inside the
 * client calls it.
 */
export function unwrap<T, E>(result: Result<T, E>): T {
  if (result.success) {
    return result.data;
  }
  throw new Error(
    `unwrap() called on a failed Result: ${describe(result.error)}`,
  );
}

/**
 * Get the value, or `fallback` if the `Result` is a failure.
 *
 * **Do not use this to render a UI.** `unwrapOr(await client.projects.list(),
 * [])` collapses "this account has no projects" and "the server is unreachable"
 * into the same empty state, which is exactly the regression the `Result` API
 * exists to make impossible: the failure is discarded one line after it was
 * produced, and nothing downstream can tell there was one.
 *
 * It is appropriate where the fallback is genuinely correct regardless of why
 * the call failed: a cached-count optimisation, a best-effort telemetry read, a
 * script that already logged the error. Anywhere a human sees the outcome,
 * branch on `result.success` and read `error.kind`.
 */
export function unwrapOr<T, E>(result: Result<T, E>, fallback: T): T {
  return result.success ? result.data : fallback;
}

/** Apply `fn` to the value of a successful `Result`, passing failures through. */
export function mapResult<T, U, E>(
  result: Result<T, E>,
  fn: (value: T) => U,
): Result<U, E> {
  return result.success ? Ok(fn(result.data)) : result;
}

/** Best-effort rendering of an unknown error value for `unwrap`'s message. */
function describe(error: unknown): string {
  if (
    typeof error === 'object' &&
    error !== null &&
    'message' in error &&
    typeof (error as { message: unknown }).message === 'string'
  ) {
    return (error as { message: string }).message;
  }
  return String(error);
}
