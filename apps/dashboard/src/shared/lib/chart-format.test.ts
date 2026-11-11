import { describe, expect, it } from 'vitest';
import { axisDuration, isDaily, percentChange } from './chart-format';

describe('axisDuration', () => {
  it('reads short on an axis', () => {
    expect(axisDuration(0)).toBe('0 ms');
    expect(axisDuration(420.6)).toBe('421 ms');
    expect(axisDuration(2000)).toBe('2 s');
    expect(axisDuration(1250)).toBe('1.3 s');
  });
});

describe('isDaily', () => {
  const at = (iso: string) => ({ bucket: iso });

  it('reads the width of the first two buckets', () => {
    expect(isDaily([at('2026-01-01T00:00Z'), at('2026-01-02T00:00Z')])).toBe(
      true,
    );
    expect(isDaily([at('2026-01-01T00:00Z'), at('2026-01-01T06:00Z')])).toBe(
      false,
    );
  });

  it('is hourly with fewer than two buckets', () => {
    expect(isDaily([at('2026-01-01T00:00Z')])).toBe(false);
  });
});

describe('percentChange', () => {
  it('has nothing to compare against an empty or absent window', () => {
    expect(percentChange(5, 0)).toBeNull();
    expect(percentChange(5, null)).toBeNull();
    expect(percentChange(15, 10)).toBe(0.5);
  });
});
