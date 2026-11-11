import { describe, expect, it } from 'vitest';
import { integerTicks, isIsolated } from './chart-scale';

describe('integerTicks', () => {
  it('steps evenly by a nice whole number from zero past the maximum', () => {
    expect(integerTicks(0)).toEqual([0, 1]);
    expect(integerTicks(1)).toEqual([0, 1]);
    expect(integerTicks(3)).toEqual([0, 1, 2, 3]);
    expect(integerTicks(5)).toEqual([0, 2, 4, 6]);
    expect(integerTicks(7)).toEqual([0, 5, 10]);
    expect(integerTicks(9)).toEqual([0, 5, 10]);
    expect(integerTicks(13)).toEqual([0, 5, 10, 15]);
    expect(integerTicks(480)).toEqual([0, 200, 400, 600]);
  });
});

describe('isIsolated', () => {
  const data = [{ v: null }, { v: 4 }, { v: null }, { v: 2 }, { v: 3 }];

  it('is a value with no value on either side', () => {
    expect(isIsolated(data, 'v', 1)).toBe(true);
    expect(isIsolated(data, 'v', 3)).toBe(false);
    expect(isIsolated(data, 'v', 0)).toBe(false);
  });

  it('counts the edges of the series as empty', () => {
    expect(isIsolated([{ v: 1 }], 'v', 0)).toBe(true);
    expect(isIsolated([{ v: 1 }, { v: null }], 'v', 0)).toBe(true);
  });
});
