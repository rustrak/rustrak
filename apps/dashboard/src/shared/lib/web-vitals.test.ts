import { describe, expect, it } from 'vitest';
import { rateVital, vitalThresholds } from './web-vitals';

describe('rateVital', () => {
  it("reads a value by web.dev's lines, good up to the first", () => {
    expect(rateVital('lcp', 2500)).toBe('good');
    expect(rateVital('lcp', 2501)).toBe('meh');
    expect(rateVital('lcp', 4001)).toBe('poor');
    expect(rateVital('constructor', 1)).toBeNull();
  });
});

describe('vitalThresholds', () => {
  it('names the two lines a vital is read by', () => {
    expect(vitalThresholds('cls')).toEqual({ good: 0.1, poor: 0.25 });
  });
});
