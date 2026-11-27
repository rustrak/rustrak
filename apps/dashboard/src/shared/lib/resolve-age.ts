/**
 * The ages auto resolve offers, in hours: Sentry's settings slider steps
 * (`getResolveAgeAllowedValues`), hourly up to 12, then every 3, 6 and 12
 * hours, then daily up to 30 days.
 */
export const RESOLVE_AGES: readonly number[] = (() => {
  const ages: number[] = [];
  let hours = 1;
  while (hours <= 720) {
    ages.push(hours);
    hours +=
      hours < 12 ? 1 : hours < 24 ? 3 : hours < 36 ? 6 : hours < 48 ? 12 : 24;
  }
  return ages;
})();

export type ResolveAgeDuration = { unit: 'hours' | 'days'; count: number };

/** An age in hours as Sentry reads it: whole days in days, else in hours. */
export function resolveAgeDuration(hours: number): ResolveAgeDuration {
  return hours > 23 && hours % 24 === 0
    ? { unit: 'days', count: hours / 24 }
    : { unit: 'hours', count: hours };
}
