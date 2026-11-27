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

/**
 * The ages a picker offers: the presets, plus `stored` in its place when the
 * API set one that is not a preset, so the picker reads it as itself.
 */
export function resolveAgeChoices(stored: number | null): readonly number[] {
  if (stored === null || RESOLVE_AGES.includes(stored)) return RESOLVE_AGES;
  return [...RESOLVE_AGES, stored].sort((a, b) => a - b);
}
