/**
 * How long a run took, at the precision a job's runtime is read at: a
 * nightly backup is "1h 5m", not "3,900,000 ms".
 */
export function formatRunDuration(ms: number): string {
  if (ms < 1_000) return `${Math.round(ms)}ms`;
  if (ms < 60_000) return `${Number((ms / 1_000).toFixed(1))}s`;
  const [big, small, bigUnit, smallUnit] =
    ms < 3_600_000
      ? [Math.floor(ms / 60_000), Math.round((ms % 60_000) / 1_000), 'm', 's']
      : [
          Math.floor(ms / 3_600_000),
          Math.round((ms % 3_600_000) / 60_000),
          'h',
          'm',
        ];
  return small ? `${big}${bigUnit} ${small}${smallUnit}` : `${big}${bigUnit}`;
}
