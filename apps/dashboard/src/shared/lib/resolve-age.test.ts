import { describe, expect, it } from 'vitest';
import { RESOLVE_AGES, resolveAgeDuration } from './resolve-age';

describe('resolveAgeDuration', () => {
  it('reads whole days in days, as Sentry does', () => {
    expect(resolveAgeDuration(24)).toEqual({ unit: 'days', count: 1 });
    expect(resolveAgeDuration(720)).toEqual({ unit: 'days', count: 30 });
  });

  it('reads anything else in hours', () => {
    expect(resolveAgeDuration(1)).toEqual({ unit: 'hours', count: 1 });
    expect(resolveAgeDuration(36)).toEqual({ unit: 'hours', count: 36 });
  });
});

describe('RESOLVE_AGES', () => {
  it("are Sentry's slider steps, from one hour to thirty days", () => {
    expect(RESOLVE_AGES.slice(0, 14)).toEqual([
      1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 15, 18,
    ]);
    expect(RESOLVE_AGES).toContain(36);
    expect(RESOLVE_AGES).toContain(48);
    expect(RESOLVE_AGES).toContain(72);
    expect(RESOLVE_AGES.at(-1)).toBe(720);
  });
});
