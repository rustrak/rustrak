/**
 * The time windows the project pages (overview, issues' releases, logs,
 * performance) are read through, so a window picked on one means the same on
 * the others. Agents and cron timelines keep their own lists on purpose.
 */
const OVERVIEW_PERIODS = ['24h', '7d', '14d', '30d'] as const;

export type OverviewPeriod = (typeof OVERVIEW_PERIODS)[number];

/** What `?period=` can hold. Absent is the default window, not all time. */
export const PERIOD_CHOICES = [...OVERVIEW_PERIODS, 'all'] as const;
export type PeriodChoice = (typeof PERIOD_CHOICES)[number];

/** The window a page opens on: two weeks, as Sentry's releases do. */
export const DEFAULT_PERIOD: OverviewPeriod = '14d';

/**
 * The windows of the performance pages. Their widest is 90 days, not all
 * time: the server reads `transactions` and `spans` over 90 days at most, as
 * Sentry does, so an "all time" there would name something it does not show.
 */
export const PERFORMANCE_PERIODS = [...OVERVIEW_PERIODS, '90d'] as const;
export type PerformancePeriod = (typeof PERFORMANCE_PERIODS)[number];

/**
 * `?period=` as a performance page reads it. `all`, from an address or a
 * saved view made when the pages offered it, is the widest window.
 */
export function performancePeriod(
  value: unknown,
): PerformancePeriod | undefined {
  if (value === 'all') return '90d';
  return PERFORMANCE_PERIODS.find((period) => period === value);
}

/** The `period` the API takes: all time is no period at all. */
export function apiPeriod(choice: PeriodChoice): OverviewPeriod | undefined {
  return choice === 'all' ? undefined : choice;
}

/** `?period=` for a chosen window: the default is left out of the address. */
export function periodParam<P extends string>(choice: P): P | undefined {
  return choice === DEFAULT_PERIOD ? undefined : choice;
}

/** A window's length in hours: `24h` is 24, `7d` is 168. */
export function periodHours(period: `${number}${'h' | 'd'}`): number {
  const value = Number.parseInt(period, 10);
  return period.endsWith('d') ? value * 24 : value;
}

/**
 * Bucket width, in hours, for a window's charts.
 *
 * A 30-day window in hourly buckets is 720 bars in a tile a few hundred pixels
 * wide, which renders as noise. Widening the bucket with the window keeps the
 * bar count at 24 to 60 for every fixed period. All time (`undefined`) takes
 * the widest; the charts also cap how many bars they draw.
 */
export function overviewInterval(period?: OverviewPeriod): number {
  switch (period) {
    case '24h':
      return 1;
    case '7d':
      return 3;
    case '14d':
      return 6;
    case '30d':
      return 12;
    default:
      return 24;
  }
}
