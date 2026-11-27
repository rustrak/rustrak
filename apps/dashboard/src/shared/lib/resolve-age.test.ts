import { describe, expect, it } from 'vitest';
import {
  RESOLVE_AGES,
  resolveAgeChoices,
  resolveAgeDuration,
} from './resolve-age';

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

describe('resolveAgeChoices', () => {
  it('are the presets when the stored age is one, or none is stored', () => {
    expect(resolveAgeChoices(168)).toEqual(RESOLVE_AGES);
    expect(resolveAgeChoices(null)).toEqual(RESOLVE_AGES);
  });

  it('add an age set through the API in its place, so it reads as itself', () => {
    const choices = resolveAgeChoices(13);
    expect(choices).toContain(13);
    expect(choices.indexOf(13)).toBe(choices.indexOf(12) + 1);
  });
});
