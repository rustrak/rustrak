import { describe, expect, it } from 'vitest';
import {
  assessment,
  firstFix,
  metricHref,
  pageHref,
  pageSpeedHref,
  ratingShares,
  ratingTrend,
  reportsWebVitals,
  scoreBand,
} from './web-vitals';

describe('reportsWebVitals', () => {
  it('is a browser JavaScript project, not a server or a mobile one', () => {
    expect(reportsWebVitals('javascript')).toBe(true);
    expect(reportsWebVitals('javascript-react')).toBe(true);
    expect(reportsWebVitals('javascript-nextjs')).toBe(true);
    expect(reportsWebVitals('node')).toBe(false);
    expect(reportsWebVitals('react-native')).toBe(false);
    expect(reportsWebVitals('electron')).toBe(false);
    expect(reportsWebVitals(null)).toBe(false);
  });
});

describe('scoreBand', () => {
  it('reads 90 and over as good and under 50 as poor, as Sentry does', () => {
    expect(scoreBand(0.9)).toBe('good');
    expect(scoreBand(0.89)).toBe('meh');
    expect(scoreBand(0.5)).toBe('meh');
    expect(scoreBand(0.49)).toBe('poor');
  });
});

describe('ratingTrend', () => {
  it('turns each bucket into the share of its loads in each band', () => {
    const [point] = ratingTrend([
      {
        bucket: '2026-11-22T10:00:00Z',
        distribution: { good: 3, meh: 1, poor: 0 },
      },
    ]);
    expect(point).toEqual({
      t: Date.parse('2026-11-22T10:00:00Z'),
      good: 75,
      meh: 25,
      poor: 0,
    });
  });

  it('leaves a bucket with no measurement out, not at 0% good', () => {
    expect(
      ratingTrend([
        {
          bucket: '2026-11-22T10:00:00Z',
          distribution: { good: 0, meh: 0, poor: 0 },
        },
      ]),
    ).toEqual([]);
  });
});

describe('pageHref', () => {
  it('keeps the page and the filters it was read under', () => {
    expect(pageHref(3, '/checkout?a', { period: '7d' })).toBe(
      '/projects/3/web-vitals/summary?name=%2Fcheckout%3Fa&period=7d',
    );
  });
});

const stats = (
  vital: 'lcp' | 'inp' | 'cls' | 'fcp' | 'ttfb',
  value: number,
) => ({
  vital,
  value,
  percentiles: [],
  score: 1,
  count: 1,
  distribution: { good: 1, meh: 0, poor: 0 },
});

describe('assessment', () => {
  it('passes when LCP, INP and CLS are all good, as PageSpeed reads it', () => {
    expect(
      assessment([stats('lcp', 2000), stats('inp', 150), stats('cls', 0.05)]),
    ).toEqual({ passed: true, failing: [] });
  });

  it('names the Core Web Vitals that fail, and ignores the others', () => {
    expect(
      assessment([stats('lcp', 3000), stats('cls', 0.3), stats('fcp', 9000)]),
    ).toEqual({ passed: false, failing: ['lcp', 'cls'] });
  });

  it('has nothing to say without a Core Web Vital', () => {
    expect(assessment([stats('fcp', 900)])).toBeNull();
  });
});

describe('ratingShares', () => {
  it('turns the counts into shares of the measurements', () => {
    expect(ratingShares({ good: 2, meh: 1, poor: 1 })).toEqual({
      good: 0.5,
      meh: 0.25,
      poor: 0.25,
    });
    expect(ratingShares({ good: 0, meh: 0, poor: 0 })).toEqual({
      good: 0,
      meh: 0,
      poor: 0,
    });
  });
});

describe('pageSpeedHref', () => {
  it('opens a lab test of the page in PageSpeed Insights', () => {
    expect(pageSpeedHref('https://shop.example.com/a?b=1')).toBe(
      'https://pagespeed.web.dev/report?url=https%3A%2F%2Fshop.example.com%2Fa%3Fb%3D1',
    );
  });
});

describe('metricHref', () => {
  it('opens one vital, keeping the page and filters it was read under', () => {
    expect(metricHref(3, 'lcp', { page: '/checkout', device: 'mobile' })).toBe(
      '/projects/3/web-vitals/lcp?page=%2Fcheckout&device=mobile',
    );
  });
});

describe('firstFix', () => {
  const page = (name: string, vitals: ReturnType<typeof stats>[]) => ({
    page: name,
    pageloads: 1,
    score: 0.5,
    opportunity: 0.1,
    vitals,
  });

  it('is the costliest page and its worst vital, poor before improvable', () => {
    expect(
      firstFix([
        page('/checkout', [
          stats('inp', 300),
          stats('lcp', 5000),
          stats('cls', 0.05),
        ]),
        page('/', [stats('lcp', 9000)]),
      ]),
    ).toEqual({ page: '/checkout', vital: 'lcp', value: 5000 });
  });

  it('weighs vitals equally rated by their weight in the score', () => {
    expect(
      firstFix([page('/a', [stats('ttfb', 1000), stats('lcp', 3000)])]),
    ).toEqual({ page: '/a', vital: 'lcp', value: 3000 });
  });

  it('has nothing to fix when every vital is good', () => {
    expect(firstFix([page('/a', [stats('lcp', 1000)])])).toBeNull();
    expect(firstFix([])).toBeNull();
  });
});
