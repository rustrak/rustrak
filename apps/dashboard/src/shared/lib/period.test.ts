import { describe, expect, it } from 'vitest';
import { apiPeriod, periodHours, periodParam } from './period';

describe('period', () => {
  it('reads hours and days', () => {
    expect(periodHours('24h')).toBe(24);
    expect(periodHours('7d')).toBe(168);
  });

  it('sends all time as no period and leaves the default out of the URL', () => {
    expect(apiPeriod('all')).toBeUndefined();
    expect(apiPeriod('7d')).toBe('7d');
    expect(periodParam('14d')).toBeUndefined();
    expect(periodParam('all')).toBe('all');
  });
});
