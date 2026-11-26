import { describe, expect, it } from 'vitest';
import {
  bounceRate,
  byKey,
  change,
  duration,
  filterOf,
  keyOf,
  narrows,
  pathOf,
  share,
  unshown,
  visitLength,
  visits,
} from './model';

describe('the traffic figures', () => {
  it('reads a bounce rate and a change against the period before', () => {
    expect(
      bounceRate({
        pageviews: 10,
        visitors: 4,
        bounces: 1,
        visits: 4,
        duration: 0,
      }),
    ).toBe(25);
    expect(
      bounceRate({
        pageviews: 0,
        visitors: 0,
        bounces: 0,
        visits: 0,
        duration: 0,
      }),
    ).toBeNull();
    expect(change(15, 10)).toBe(50);
    expect(change(5, 10)).toBe(-50);
    expect(change(5, 0)).toBeNull();
  });

  it("finds a failing page's path and a DSN's key", () => {
    expect(pathOf('https://shop.example.com/cart?x=1')).toBe('/cart');
    expect(pathOf('nonsense')).toBeNull();
    expect(pathOf(undefined)).toBeNull();
    expect(keyOf('https://abc123@rustrak.example.com/3')).toBe('abc123');
    expect(keyOf(null)).toBeNull();
  });
});

describe('the narrowing', () => {
  it('keeps the dimensions it knows and a valid window', () => {
    expect(filterOf({ country: 'ES', period: '30d', tab: 'x' })).toEqual({
      period: '30d',
      country: 'ES',
    });
    expect(filterOf({ period: 'all' })).toEqual({ period: '14d' });
  });

  it('narrows an entry or exit page as a page, and an event by nothing', () => {
    expect(narrows('entry')).toBe('path');
    expect(narrows('path')).toBe('path');
    expect(narrows('country')).toBe('country');
    expect(narrows('event')).toBeNull();
  });
});

describe('visits and events', () => {
  it('reads an average visit', () => {
    expect(visitLength({ visits: 2, duration: 130 })).toBe(65);
    expect(visitLength({ visits: 0, duration: 0 })).toBeNull();
    expect(duration(65)).toBe('1m 5s');
    expect(duration(3725)).toBe('1h 2m');
    expect(duration(9)).toBe('9s');
  });

  it("groups an event's properties by key, in order", () => {
    expect(
      byKey([
        { key: 'plan', value: 'pro', count: 2 },
        { key: 'plan', value: 'free', count: 1 },
        { key: 'seats', value: '3', count: 1 },
      ]).map(([k, vs]) => [k, vs.length]),
    ).toEqual([
      ['plan', 2],
      ['seats', 1],
    ]);
  });
});

describe("a visitor's day", () => {
  const step = (at: number) => ({
    kind: 'pageview' as const,
    at,
    path: '/',
    label: null,
    data: null,
  });

  it('splits into visits at a half-hour pause', () => {
    const day = [step(0), step(60), step(60 + 31 * 60), step(60 + 32 * 60)];
    expect(visits(day).map((v) => v.length)).toEqual([2, 2]);
    expect(visits([])).toEqual([]);
  });

  it('splits at a pause between pages, as the totals do, whatever events fall in it', () => {
    const event = (at: number) => ({ ...step(at), kind: 'event' as const });
    // Pages at 12:00 and 12:40, an event at 12:20: two visits.
    const day = [step(0), event(20 * 60), step(40 * 60)];
    expect(visits(day).map((v) => v.length)).toEqual([2, 1]);
    // A day of events alone splits at a pause between them.
    expect(
      visits([event(0), event(60), event(2 * 3600)]).map((v) => v.length),
    ).toEqual([2, 1]);
  });

  it('counts the steps a long day leaves out of its list', () => {
    const day = [step(0), step(60)];
    expect(unshown(day, { pageviews: 600, events: 1 })).toBe(599);
    expect(unshown(day, { pageviews: 1, events: 1 })).toBe(0);
  });
});

describe('shares', () => {
  it('reads a share', () => {
    expect(share(1, 4)).toBe(25);
    expect(share(1, 0)).toBeNull();
  });
});
