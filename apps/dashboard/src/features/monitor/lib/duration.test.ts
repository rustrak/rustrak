import { describe, expect, it } from 'vitest';
import { formatRunDuration } from './duration';

describe('formatRunDuration', () => {
  it('scales from milliseconds to hours', () => {
    expect(formatRunDuration(850)).toBe('850ms');
    expect(formatRunDuration(12_500)).toBe('12.5s');
    expect(formatRunDuration(200_000)).toBe('3m 20s');
    expect(formatRunDuration(3_900_000)).toBe('1h 5m');
  });

  it('drops a zero remainder', () => {
    expect(formatRunDuration(120_000)).toBe('2m');
    expect(formatRunDuration(7_200_000)).toBe('2h');
  });
});
