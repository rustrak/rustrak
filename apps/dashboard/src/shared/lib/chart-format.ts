/**
 * Relative change from `previous` to `current`.
 *
 * Returns `null` when there is nothing to compare against: no previous window
 * at all (an all-time request), or a previous window of zero, where any
 * increase is an undefined percentage rather than "+100%".
 */
export function percentChange(
  current: number,
  previous: number | null,
): number | null {
  if (previous === null || previous === 0) {
    return null;
  }
  return (current - previous) / previous;
}

/** Whether a series' buckets are a day or wider, read from its first two. */
export function isDaily(buckets: readonly { bucket: string }[]): boolean {
  const [first, second] = buckets;
  return first && second
    ? Date.parse(second.bucket) - Date.parse(first.bucket) >= 86_400_000
    : false;
}

/** A duration on a chart axis: whole milliseconds, or seconds to one place. */
export function axisDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  return `${Number((ms / 1000).toFixed(1))} s`;
}
