import { describe, expect, it } from 'vitest';
import {
  apiPeriod,
  performancePeriod,
  periodHours,
  periodParam,
} from './period';

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

describe('performancePeriod', () => {
  it('reads the windows the performance pages offer', () => {
    expect(performancePeriod('24h')).toBe('24h');
    expect(performancePeriod('90d')).toBe('90d');
  });

  it('reads all time, from an older address or saved view, as the widest', () => {
    expect(performancePeriod('all')).toBe('90d');
  });

  it('leaves anything else to the default', () => {
    expect(performancePeriod(undefined)).toBeUndefined();
    expect(performancePeriod('365d')).toBeUndefined();
  });
});
