import { describe, expect, it } from 'vitest';
import { parseEpochSeconds } from './protocol-timestamp';

describe('parseEpochSeconds', () => {
  it('passes a finite number through as epoch seconds', () => {
    expect(parseEpochSeconds(1789687315.213)).toBe(1789687315.213);
    expect(parseEpochSeconds(0)).toBe(0);
  });

  it('rejects a number that is not a moment', () => {
    expect(parseEpochSeconds(Number.NaN)).toBeUndefined();
    expect(parseEpochSeconds(Number.POSITIVE_INFINITY)).toBeUndefined();
  });

  // The exact shape sentry-python writes: `strftime("%Y-%m-%dT%H:%M:%S.%fZ")`,
  // six fractional digits and a literal `Z`.
  it('reads the RFC 3339 string sentry-python sends, keeping microseconds', () => {
    expect(parseEpochSeconds('2026-09-17T23:21:54.326158Z')).toBe(
      1789687314 + 0.326158,
    );
  });

  it('reads the Relay documentation example', () => {
    expect(parseEpochSeconds('2016-04-20T20:55:53.845Z')).toBe(
      1461185753 + 0.845,
    );
  });

  it('reads the lowercase separator and zulu marker chrono accepts', () => {
    expect(parseEpochSeconds('1970-01-01t00:16:42z')).toBe(1002);
    expect(parseEpochSeconds('2016-04-20t20:55:53.845Z')).toBe(
      1461185753 + 0.845,
    );
  });

  it('treats a missing offset as UTC, as Relay does', () => {
    expect(parseEpochSeconds('1970-01-01T00:16:42')).toBe(1002);
    expect(parseEpochSeconds('1970-01-01 00:16:42.5')).toBe(1002.5);
  });

  it('honours an explicit offset', () => {
    expect(parseEpochSeconds('1970-01-01T02:00:00+02:00')).toBe(0);
    expect(parseEpochSeconds('1970-01-01T02:00:00+0200')).toBe(0);
  });

  it('rejects what chrono rejects rather than letting Date.parse guess', () => {
    expect(parseEpochSeconds('not-a-timestamp')).toBeUndefined();
    // A calendar date that does not exist rolls into March under Date.parse.
    expect(parseEpochSeconds('2026-02-30T00:00:00Z')).toBeUndefined();
    // 24:00 is midnight tomorrow to Date.parse, and invalid to chrono.
    expect(parseEpochSeconds('2026-01-01T24:00:00Z')).toBeUndefined();
    // Bare epoch digits in a string are not RFC 3339.
    expect(parseEpochSeconds('1789687315')).toBeUndefined();
  });

  it('is undefined for anything that is neither number nor string', () => {
    expect(parseEpochSeconds(undefined)).toBeUndefined();
    expect(parseEpochSeconds(null)).toBeUndefined();
    expect(parseEpochSeconds({ seconds: 1 })).toBeUndefined();
    expect(parseEpochSeconds(true)).toBeUndefined();
  });
});
