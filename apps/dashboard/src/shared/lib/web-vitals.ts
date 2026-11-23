/**
 * How a web vital's value reads, by web.dev's lines: the published "good"
 * and "poor" thresholds every browser team shares. Sentry colours its cards
 * by its score curve's points instead (LCP good under 1.2s); these are the
 * ones a reader can look up. In ms, but CLS. The server counts ratings by
 * the same lines (`Vital::thresholds`, apps/server/src/services/web_vitals.rs).
 */
const THRESHOLDS: Record<string, { good: number; poor: number }> = {
  lcp: { good: 2500, poor: 4000 },
  fcp: { good: 1800, poor: 3000 },
  fid: { good: 100, poor: 300 },
  inp: { good: 200, poor: 500 },
  ttfb: { good: 800, poor: 1800 },
  cls: { good: 0.1, poor: 0.25 },
};

export type VitalRating = 'good' | 'meh' | 'poor';

/** A vital's rating, or `null` for a measurement that is not a vital. */
export function rateVital(key: string, value: number): VitalRating | null {
  const name = key.toLowerCase();
  // Own keys only: `constructor` is not a vital.
  if (!Object.hasOwn(THRESHOLDS, name)) return null;
  const { good, poor } = THRESHOLDS[name];
  if (value <= good) return 'good';
  return value <= poor ? 'meh' : 'poor';
}

/** The two lines a vital is read by: good up to `good`, poor past `poor`. */
export function vitalThresholds(vital: string): { good: number; poor: number } {
  return THRESHOLDS[vital];
}

/** The text colour of each rating. */
export const RATING_TEXT: Record<VitalRating, string> = {
  good: 'text-success-fg',
  meh: 'text-sev-warning-fg',
  poor: 'text-sev-error-fg',
};
