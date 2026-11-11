/**
 * The current window's series, each bucket paired with the total of the
 * bucket exactly one window before it, read from a series fetched over twice
 * the window.
 *
 * The current values come from their own request, not from the doubled
 * one: the window starts mid-bucket, and the doubled series' bucket of that
 * hour holds the whole hour, events the current window leaves out. The
 * doubled window is zero-filled from `now - 2P`, so the bucket a period
 * before the current window's `i`-th is the doubled series' `i`-th.
 */
export function withPreviousPeriod<T extends { total: number }>(
  current: T[],
  doubled: { total: number }[],
): (T & { previous: number })[] {
  return current.map((point, index) => ({
    ...point,
    previous: doubled[index]?.total ?? 0,
  }));
}
