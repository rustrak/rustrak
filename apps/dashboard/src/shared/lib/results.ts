import type { Result, RustrakError } from '@rustrak/client';
import { Ok } from '@rustrak/client';

type PendingResult = Promise<Result<unknown, RustrakError>>;

/**
 * The success type of one `Result`.
 *
 * Written as a conditional over the success member alone, not over
 * `Result<infer U, E>`. `Result` is a union, and inferring a union through a
 * union is where TypeScript gives up and hands back the widened element type
 * instead of the tuple member.
 */
type OkData<R> = R extends { success: true; data: infer U } ? U : never;

/** The success types of a tuple of pending `Result`s, in the same order. */
type Unwrapped<T extends readonly PendingResult[]> = {
  -readonly [K in keyof T]: OkData<Awaited<T[K]>>;
};

/**
 * Await several fetches together and collapse them into one `Result`, keeping
 * the first failure.
 *
 * For the pages that need more than one fetch before they can render anything.
 * The alternative is an `if (!x.success)` per fetch, which is correct but so
 * tedious that discarding the failure and rendering an empty state starts to
 * look reasonable. This keeps the failure, keeps the narrowing, and reads as one
 * decision:
 *
 * ```tsx
 * const loaded = await loadAll([getProject(id), listIssues(id)]);
 * if (!loaded.success) {
 *   return <LoadFailure error={loaded.error} title="Could not load issues" />;
 * }
 * const [project, issues] = loaded.data; // both narrowed, no cast
 * ```
 *
 * It takes the promises rather than the awaited results on purpose: an inline
 * `Promise.all([...])` passed as an argument gets contextually typed by this
 * signature and widens to an array of the union of its members, losing the
 * tuple. Owning the `Promise.all` keeps the array literal's own inference, and
 * as a bonus the calls still run concurrently.
 *
 * Only the first failure is reported. A page renders one surface, so a second
 * reason would have nowhere to go.
 *
 * Use it only where every member is genuinely required. A fetch the page can do
 * without stays its own `result.success ? result.data : fallback`, so that the
 * degradation stays visible at the line that chose it.
 */
export async function loadAll<T extends readonly PendingResult[] | []>(
  // The `| []` in the constraint is the standard trick (the one `Promise.all`'s
  // own typings use) that makes TypeScript infer a tuple here rather than an
  // array of the union of the members.
  pending: T,
): Promise<Result<Unwrapped<T>, RustrakError>> {
  const results = await Promise.all(pending);
  const data: unknown[] = [];

  for (const result of results) {
    if (!result.success) {
      return result;
    }
    data.push(result.data);
  }

  return Ok(data as Unwrapped<T>);
}

/**
 * The first failure among results a page needs, skipping the ones it did not
 * ask for (`null`): a tab's read is only made while the tab is open.
 */
export function firstFailure(
  results: readonly (Result<unknown, RustrakError> | null)[],
): RustrakError | undefined {
  for (const result of results) {
    if (result && !result.success) return result.error;
  }
  return undefined;
}

/** A result's data, or `null` for a failure or a read never made. */
export function okOrNull<T>(result: Result<T, RustrakError> | null): T | null {
  return result?.success ? result.data : null;
}

/**
 * A read the page can do without: its data, or `fallback` when it failed.
 * Never a failure, so a deferred panel that lost its read degrades the way it
 * did when the loader awaited it, rather than showing an error.
 */
export async function orElse<T, F>(
  pending: Promise<Result<T, RustrakError>>,
  fallback: F,
): Promise<Result<T | F, RustrakError>> {
  const result = await pending;
  return Ok(result.success ? result.data : fallback);
}
