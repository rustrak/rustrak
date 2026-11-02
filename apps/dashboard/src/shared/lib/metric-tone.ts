/**
 * Whether a rise in this metric is good or bad news.
 *
 * Callers must say: more events is bad, more resolved issues is good. Colour
 * the arrow by direction alone and it tells the reader the opposite of the
 * truth half the time.
 */
export type Polarity = 'up-is-bad' | 'up-is-good';

/**
 * The text colour a period-over-period change should carry, given what
 * direction means for that metric.
 *
 * Lives here rather than beside `MetricDeltaText` because `StatTile` needs it
 * too, and a module that exports both a component and a helper cannot keep
 * its state across a Fast Refresh.
 */
export function deltaTone(change: number, polarity: Polarity): string {
  if (change === 0) {
    return 'text-fg-subtle';
  }
  const isGood = polarity === 'up-is-good' ? change > 0 : change < 0;
  return isGood ? 'text-success-fg' : 'text-sev-error-fg';
}
