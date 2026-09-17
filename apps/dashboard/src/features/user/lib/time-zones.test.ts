import { describe, expect, it } from 'vitest';
import {
  formatOffset,
  listTimeZones,
  timeZoneOffsetMinutes,
} from './time-zones';

// A summer instant and a winter one, so DST shows up in the offsets.
const july = new Date('2026-07-01T12:00:00Z');
const january = new Date('2026-01-15T12:00:00Z');

describe('timeZoneOffsetMinutes', () => {
  it('is zero for UTC whatever the season', () => {
    expect(timeZoneOffsetMinutes('UTC', july)).toBe(0);
    expect(timeZoneOffsetMinutes('UTC', january)).toBe(0);
  });

  it('follows daylight saving time', () => {
    expect(timeZoneOffsetMinutes('Europe/Madrid', july)).toBe(120);
    expect(timeZoneOffsetMinutes('Europe/Madrid', january)).toBe(60);
  });

  it('handles zones west of Greenwich and half-hour offsets', () => {
    expect(timeZoneOffsetMinutes('America/New_York', january)).toBe(-300);
    expect(timeZoneOffsetMinutes('Asia/Kolkata', january)).toBe(330);
    expect(timeZoneOffsetMinutes('America/St_Johns', january)).toBe(-210);
  });

  it('is unaffected by the milliseconds on the instant', () => {
    const withMillis = new Date('2026-01-15T12:00:00.987Z');
    expect(timeZoneOffsetMinutes('Asia/Tokyo', withMillis)).toBe(540);
  });
});

describe('formatOffset', () => {
  it('labels zero as plain UTC', () => {
    expect(formatOffset(0)).toBe('UTC');
  });

  it('pads hours and minutes and keeps the sign', () => {
    expect(formatOffset(120)).toBe('UTC+02:00');
    expect(formatOffset(-210)).toBe('UTC-03:30');
    expect(formatOffset(345)).toBe('UTC+05:45');
  });
});

describe('listTimeZones', () => {
  const options = listTimeZones(january);

  it('offers UTC, so it can be chosen on purpose', () => {
    expect(options.some((option) => option.value === 'UTC')).toBe(true);
  });

  it('offers only names the runtime can format with', () => {
    const supported = new Set(Intl.supportedValuesOf('timeZone'));
    for (const option of options) {
      if (option.value === 'UTC') continue;
      expect(supported.has(option.value)).toBe(true);
    }
  });

  it('sorts west to east, then by name', () => {
    for (let i = 1; i < options.length; i++) {
      const previous = options[i - 1];
      const current = options[i];
      expect(
        previous.offsetMinutes < current.offsetMinutes ||
          (previous.offsetMinutes === current.offsetMinutes &&
            previous.value.localeCompare(current.value) <= 0),
      ).toBe(true);
    }
  });

  it('labels each option with its offset at the given instant', () => {
    const madrid = options.find((option) => option.value === 'Europe/Madrid');
    expect(madrid).toEqual({
      value: 'Europe/Madrid',
      offsetMinutes: 60,
      offset: 'UTC+01:00',
    });
  });
});
