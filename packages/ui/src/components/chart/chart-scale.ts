/**
 * Scale helpers shared by the charts. Plain values, beside the components
 * for the same Fast Refresh reason as `chart-series.ts`.
 */

/**
 * Evenly spaced whole-number ticks from zero to the first one at or past
 * `max`, at most `count` of them, stepping by 1, 2 or 5 × 10ⁿ. Recharts'
 * own integer ticks round each value and drop the ones it decides overlap,
 * which reads as `0, 1, 3`.
 */
export function integerTicks(max: number, count = 4): number[] {
  const top = Math.max(1, Math.ceil(max));
  for (let magnitude = 1; ; magnitude *= 10) {
    for (const step of [magnitude, 2 * magnitude, 5 * magnitude]) {
      if (step * (count - 1) >= top) {
        return Array.from(
          { length: Math.ceil(top / step) + 1 },
          (_, i) => i * step,
        );
      }
    }
  }
}

const empty = (value: unknown) => value == null;

/**
 * A point with no value on either side draws no line at all, so it is the
 * one point that needs a resting dot to be seen.
 */
export function isIsolated(
  data: Record<string, unknown>[],
  key: string,
  index: number,
): boolean {
  return (
    !empty(data[index]?.[key]) &&
    empty(data[index - 1]?.[key]) &&
    empty(data[index + 1]?.[key])
  );
}

/** The highest value drawn: the summed band when the series stack. */
export function dataMax(
  data: Record<string, unknown>[],
  keys: string[],
  stacked: boolean,
): number {
  let max = 0;
  for (const datum of data) {
    const values = keys.map((key) => Number(datum[key]) || 0);
    max = Math.max(
      max,
      stacked ? values.reduce((a, b) => a + b, 0) : Math.max(0, ...values),
    );
  }
  return max;
}
