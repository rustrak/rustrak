import type {
  RatingCounts,
  Vital,
  VitalStats,
  WebVitalsPage,
  WebVitalsPoint,
} from '@rustrak/client';
import { rateVital } from '@/shared/lib/web-vitals';

/**
 * Whether a project's SDK runs in a browser, where web vitals are measured:
 * JavaScript and its frameworks. A server, a phone or Electron has none.
 */
export function reportsWebVitals(platform: string | null): boolean {
  return (
    platform === 'javascript' || (platform?.startsWith('javascript-') ?? false)
  );
}

/** How a performance score reads, with Sentry's cut-offs at 90 and 50. */
export function scoreBand(score: number): 'good' | 'meh' | 'poor' {
  if (score >= 0.9) return 'good';
  if (score >= 0.5) return 'meh';
  return 'poor';
}

/** The text colour of each band. */
export const BAND_TEXT = {
  good: 'text-success-fg',
  meh: 'text-sev-warning-fg',
  poor: 'text-sev-error-fg',
} as const;

/** Each vital's weight in the score, as the server weighs it. */
export const VITAL_WEIGHTS: Record<Vital, number> = {
  lcp: 0.3,
  inp: 0.3,
  cls: 0.15,
  fcp: 0.15,
  ttfb: 0.1,
};

export const VITALS = Object.keys(VITAL_WEIGHTS) as Vital[];

/** Each vital's abbreviation and its name, which stay in English everywhere. */
export const VITAL_NAMES: Record<Vital, { short: string; long: string }> = {
  lcp: { short: 'LCP', long: 'Largest Contentful Paint' },
  inp: { short: 'INP', long: 'Interaction to Next Paint' },
  cls: { short: 'CLS', long: 'Cumulative Layout Shift' },
  fcp: { short: 'FCP', long: 'First Contentful Paint' },
  ttfb: { short: 'TTFB', long: 'Time to First Byte' },
};

/** The three Core Web Vitals, the ones a page passes or fails on. */
export const CORE_VITALS: readonly Vital[] = ['lcp', 'inp', 'cls'];

/** The two that lead to them, read on their own. */
export const OTHER_VITALS: readonly Vital[] = ['fcp', 'ttfb'];

/**
 * Each bucket as the share of its measurements in each band, out of 100:
 * stacked, they fill the bar, as CrUX draws a metric's trend. A bucket with
 * none is left out: drawn, it would read as 0% good.
 */
export function ratingTrend(
  points: WebVitalsPoint[],
): { t: number; good: number; meh: number; poor: number }[] {
  return points.flatMap((point) => {
    const { good, meh, poor } = point.distribution;
    const total = good + meh + poor;
    if (total === 0) return [];
    const share = (n: number) => (n / total) * 100;
    return [
      {
        t: Date.parse(point.bucket),
        good: share(good),
        meh: share(meh),
        poor: share(poor),
      },
    ];
  });
}

/** One page's web vitals, keeping the filters it was read under. */
export function pageHref(
  projectId: number,
  page: string,
  scope: {
    period?: string;
    environment?: string;
    browser?: string;
    device?: string;
  } = {},
): string {
  const params = new URLSearchParams({ name: page });
  for (const key of ['period', 'environment', 'browser', 'device'] as const) {
    const value = scope[key];
    if (value) params.set(key, value);
  }
  return `/projects/${projectId}/web-vitals/summary?${params}`;
}

/**
 * The Core Web Vitals assessment as PageSpeed Insights gives it: passed when
 * each of LCP, INP and CLS is good at its 75th percentile. One without data
 * is left out of it, as CrUX leaves out a metric it has too little of;
 * `null` with none of the three.
 */
export function assessment(
  vitals: readonly VitalStats[],
): { passed: boolean; failing: Vital[] } | null {
  const core = vitals.filter((v) => CORE_VITALS.includes(v.vital));
  if (core.length === 0) return null;
  const failing = core
    .filter((v) => rateVital(v.vital, v.value) !== 'good')
    .map((v) => v.vital);
  return { passed: failing.length === 0, failing };
}

/** Each rating's share of the measurements, 0 to 1. */
export function ratingShares(counts: RatingCounts): RatingCounts {
  const total = counts.good + counts.meh + counts.poor;
  const share = (n: number) => (total === 0 ? 0 : n / total);
  return {
    good: share(counts.good),
    meh: share(counts.meh),
    poor: share(counts.poor),
  };
}

/** A lab test of a page in PageSpeed Insights: what the field data cannot say. */
export function pageSpeedHref(url: string): string {
  return `https://pagespeed.web.dev/report?url=${encodeURIComponent(url)}`;
}

/** The filters a Web Vitals address keeps from one page to the next. */
export interface VitalsScope {
  period?: string;
  environment?: string;
  browser?: string;
  device?: string;
}

/** One vital's own page, for a page or the whole project, keeping the filters. */
export function metricHref(
  projectId: number,
  vital: Vital,
  scope: VitalsScope & { page?: string } = {},
): string {
  const params = new URLSearchParams();
  for (const key of [
    'page',
    'period',
    'environment',
    'browser',
    'device',
  ] as const) {
    const value = scope[key];
    if (value) params.set(key, value);
  }
  const query = params.toString();
  return `/projects/${projectId}/web-vitals/${vital}${query ? `?${query}` : ''}`;
}

const SEVERITY = { poor: 2, meh: 1, good: 0 } as const;

/**
 * Where to start: the page that costs the project the most score (the
 * first of `pages`, which come ranked by it) and its worst vital, poor
 * before improvable and, between two alike, the one that weighs more.
 * `null` when that page has nothing below good.
 */
export function firstFix(
  pages: readonly WebVitalsPage[],
): { page: string; vital: Vital; value: number } | null {
  const [top] = pages;
  if (!top) return null;
  const worst = top.vitals
    .map((stats) => ({
      stats,
      severity: SEVERITY[rateVital(stats.vital, stats.value) ?? 'good'],
    }))
    .filter(({ severity }) => severity > 0)
    .sort(
      (a, b) =>
        b.severity - a.severity ||
        VITAL_WEIGHTS[b.stats.vital] - VITAL_WEIGHTS[a.stats.vital],
    )[0];
  return worst
    ? { page: top.page, vital: worst.stats.vital, value: worst.stats.value }
    : null;
}
