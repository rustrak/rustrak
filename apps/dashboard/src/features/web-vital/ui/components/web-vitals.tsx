import type {
  Vital,
  VitalStats,
  WebVitalSample,
  WebVitalsBreakdownRow,
  WebVitalsListPagesResponse,
  WebVitalsPage,
  WebVitalsPoint,
  WebVitalsSummary,
  WebVitalTarget,
} from '@rustrak/client';
import {
  BarsChart,
  ChevronRightIcon,
  cn,
  createDataTableColumnHelper,
  DataTable,
  type DataTableQuery,
  ExternalLinkIcon,
  formatSpanDuration,
  useDataTable,
} from '@rustrak/ui';
import { type ReactNode, useMemo } from 'react';
import { useTranslations } from 'use-intl';
import {
  assessment,
  BAND_TEXT,
  ratingShares,
  ratingTrend,
  scoreBand,
  VITAL_NAMES,
  VITALS,
} from '@/features/web-vital/model/web-vitals';
import { isDaily } from '@/shared/lib/chart-format';
import {
  RATING_TEXT,
  rateVital,
  type VitalRating,
  vitalThresholds,
} from '@/shared/lib/web-vitals';
import { Link } from '@/shared/ui/components/link';
import { RelativeTime } from '@/shared/ui/components/relative-time';
import { useBucketLabel } from '@/shared/ui/hooks/use-bucket-label';
import { useStableFormatter } from '@/shared/ui/hooks/use-stable-formatter';

/** A vital's value as it is read: CLS unitless, the others as a duration. */
function formatVital(vital: Vital, value: number): string {
  return vital === 'cls' ? value.toFixed(2) : formatSpanDuration(value);
}

/** A score from 0 to 1 as Sentry and Lighthouse print it, out of 100. */
const outOf100 = (score: number) => Math.round(score * 100);

const BANDS: readonly VitalRating[] = ['good', 'meh', 'poor'];

/** Each rating's fill, for the bars and shapes that carry it. */
const RATING_FILL: Record<VitalRating, string> = {
  good: 'text-success',
  meh: 'text-sev-warning',
  poor: 'text-sev-error',
};

/**
 * A rating as Lighthouse draws it: a circle, a square or a triangle as well
 * as a colour, so it reads without telling green from red.
 */
export function RatingShape({
  rating,
  className,
}: {
  rating: VitalRating;
  className?: string;
}) {
  return (
    <svg
      viewBox="0 0 10 10"
      aria-hidden
      className={cn(
        'size-2.5 shrink-0 fill-current',
        RATING_FILL[rating],
        className,
      )}
    >
      {rating === 'good' ? <circle cx="5" cy="5" r="5" /> : null}
      {rating === 'meh' ? <rect x="0.5" y="0.5" width="9" height="9" /> : null}
      {rating === 'poor' ? <path d="M5 0.5 9.8 9.5H0.2Z" /> : null}
    </svg>
  );
}

/** A vital's value, with its rating's shape and colour. */
function Rated({ vital, value }: { vital: Vital; value: number }) {
  const rating = rateVital(vital, value);
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1.5 whitespace-nowrap tabular-nums',
        rating ? RATING_TEXT[rating] : '',
      )}
    >
      {rating ? <RatingShape rating={rating} className="size-2" /> : null}
      {formatVital(vital, value)}
    </span>
  );
}

/**
 * The performance score as Lighthouse draws it: a ring filled to the score,
 * in its band's colour, the number inside.
 */
export function ScoreGauge({ score }: { score: number | null }) {
  const t = useTranslations('webVitals');
  const band = score == null ? null : scoreBand(score);
  const circumference = 2 * Math.PI * 26;
  return (
    <div
      className="relative size-16 shrink-0"
      role="img"
      aria-label={
        score == null ? t('noData') : `${t('score')}: ${outOf100(score)}`
      }
    >
      <svg viewBox="0 0 64 64" className="-rotate-90 size-full" aria-hidden>
        <circle
          cx="32"
          cy="32"
          r="26"
          strokeWidth="5"
          className="fill-none stroke-current text-surface-chip"
        />
        {score != null && band ? (
          <circle
            cx="32"
            cy="32"
            r="26"
            strokeWidth="5"
            strokeLinecap="round"
            strokeDasharray={`${score * circumference} ${circumference}`}
            className={cn(
              'fill-none stroke-current',
              RATING_FILL[band === 'good' ? 'good' : band],
            )}
          />
        ) : null}
      </svg>
      <span
        className={cn(
          'absolute inset-0 flex items-center justify-center text-section tabular-nums',
          band ? BAND_TEXT[band] : 'text-fg-ghost',
        )}
      >
        {score == null ? '–' : outOf100(score)}
      </span>
    </div>
  );
}

/**
 * The head of a Web Vitals page, as PageSpeed Insights opens its field
 * data: the score, and whether the Core Web Vitals pass, in one sentence.
 * A card like every other block on the page, with room on its right for
 * what to do about it.
 */
export function Verdict({
  summary,
  aside,
}: {
  summary: WebVitalsSummary;
  /** Beside the verdict, past a hairline: where to start. */
  aside?: ReactNode;
}) {
  const t = useTranslations('webVitals');
  const tVerdict = useTranslations('webVitals.assessment');
  const format = useStableFormatter();
  const verdict = assessment(summary.vitals);
  const band = summary.score == null ? null : scoreBand(summary.score);
  return (
    <section className="flex shrink-0 flex-col rounded-2xl border border-border-subtle bg-surface lg:flex-row">
      <div className="flex items-center gap-4 p-5 lg:min-w-96">
        <ScoreGauge score={summary.score} />
        <div className="flex min-w-0 flex-col gap-1">
          <p className="flex flex-wrap items-center gap-x-2 text-fg text-section">
            {verdict ? (
              <>
                <RatingShape
                  rating={verdict.passed ? 'good' : 'poor'}
                  className="size-3"
                />
                {verdict.passed ? tVerdict('passed') : tVerdict('failed')}
              </>
            ) : (
              t('score')
            )}
          </p>
          <p className="text-fg-subtle text-meta">
            {verdict
              ? verdict.passed
                ? tVerdict('passedHint')
                : tVerdict('failedHint', {
                    vitals: format.list(
                      verdict.failing.map((v) => VITAL_NAMES[v].short),
                      { type: 'conjunction' },
                    ),
                    count: verdict.failing.length,
                  })
              : band
                ? t('scoreHint', { band: t(`bands.${band}`) })
                : t('noData')}
          </p>
        </div>
      </div>
      {aside ? (
        <div className="flex min-w-0 flex-1 border-border-divider border-t lg:border-t-0 lg:border-l">
          {aside}
        </div>
      ) : null}
    </section>
  );
}

/** A threshold as it is read: `2.5 s`, `200 ms`, `0.1`. */
function useThreshold() {
  const format = useStableFormatter();
  return (vital: Vital, value: number) => {
    if (vital === 'cls') return format.number(value);
    return value >= 1000
      ? `${format.number(value / 1000, { maximumFractionDigits: 1 })} s`
      : `${format.number(value)} ms`;
  };
}

/**
 * How a vital's measurements split between the three bands, as PageSpeed
 * draws a metric: one bar, each band's share of the page loads, and a mark
 * at the 75th percentile, where the vital is judged.
 */
export function DistributionBar({ stats }: { stats: VitalStats }) {
  return (
    <div className="relative pt-1">
      <ShareBar counts={stats.distribution} className="h-2" />
      <span
        aria-hidden
        className="-translate-x-1/2 absolute top-0 left-3/4 h-4 w-0.5 rounded-pill bg-fg"
      />
    </div>
  );
}

/** Each band's share of some measurements, as one bar. */
function ShareBar({
  counts,
  className,
}: {
  counts: WebVitalTarget['distribution'];
  className: string;
}) {
  const shares = ratingShares(counts);
  return (
    <div
      className={cn(
        'flex w-full gap-px overflow-hidden rounded-pill',
        className,
      )}
    >
      {BANDS.map((band) =>
        shares[band] > 0 ? (
          <span
            key={band}
            className={cn('h-full bg-current', RATING_FILL[band])}
            style={{ width: `${shares[band] * 100}%` }}
          />
        ) : null,
      )}
    </div>
  );
}

/**
 * A vital as PageSpeed reads one: its 75th percentile with its rating, the
 * bar of how its page loads split, and each band's share beside the line
 * that bounds it.
 */
export function VitalReading({
  vital,
  stats,
}: {
  vital: Vital;
  stats: VitalStats;
}) {
  const t = useTranslations('webVitals');
  const format = useStableFormatter();
  const threshold = useThreshold();
  const lines = vitalThresholds(vital);
  const shares = ratingShares(stats.distribution);
  const rating = rateVital(vital, stats.value) ?? 'good';
  const range: Record<VitalRating, string> = {
    good: `≤ ${threshold(vital, lines.good)}`,
    meh: `${threshold(vital, lines.good)} – ${threshold(vital, lines.poor)}`,
    poor: `> ${threshold(vital, lines.poor)}`,
  };
  return (
    <div className="flex flex-col gap-4">
      <p className="flex items-baseline gap-2">
        <span
          className={cn(
            'flex items-center gap-2 text-page-title tabular-nums',
            RATING_TEXT[rating],
          )}
        >
          <RatingShape rating={rating} className="size-3" />
          {formatVital(vital, stats.value)}
        </span>
        <span className="text-fg-subtle text-meta">{t('atP75')}</span>
      </p>
      <DistributionBar stats={stats} />
      <ul className="flex flex-col gap-1.5 text-meta">
        {BANDS.map((band) => (
          <li key={band} className="flex items-center gap-2">
            <RatingShape rating={band} className="size-2" />
            <span className="text-fg-secondary">{t(`ratings.${band}`)}</span>
            <span className="text-fg-subtle tabular-nums">{range[band]}</span>
            <span className="ml-auto text-fg tabular-nums">
              {format.number(shares[band], {
                style: 'percent',
                maximumFractionDigits: 0,
              })}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * One vital on a Web Vitals page: its name and its reading, and a footer
 * that is plainly a link to everything behind it.
 */
export function MetricCard({
  vital,
  stats,
  href,
}: {
  vital: Vital;
  stats?: VitalStats;
  href: string;
}) {
  const t = useTranslations('webVitals');
  return (
    <section className="flex min-w-0 flex-col rounded-2xl border border-border-subtle bg-surface">
      <div className="flex flex-1 flex-col gap-4 p-5">
        <h3 className="flex min-w-0 items-baseline gap-2">
          <span className="font-medium text-fg text-section">
            {VITAL_NAMES[vital].short}
          </span>
          <span className="truncate text-fg-subtle text-meta">
            {VITAL_NAMES[vital].long}
          </span>
        </h3>
        {stats ? (
          <VitalReading vital={vital} stats={stats} />
        ) : (
          <p className="text-fg-subtle text-meta">{t('noDataFor')}</p>
        )}
      </div>
      {stats ? (
        <Link
          href={href}
          className="flex items-center justify-between gap-2 rounded-b-2xl border-border-divider border-t px-5 py-3 text-fg-secondary text-meta hover:bg-surface-hover hover:text-fg"
        >
          {t('analyse', { vital: VITAL_NAMES[vital].short })}
          <ChevronRightIcon size="md" />
        </Link>
      ) : null}
    </section>
  );
}

/**
 * The spread around the 75th percentile: half the page loads were faster
 * than the 50th, one in a hundred slower than the 99th.
 */
export function PercentileList({
  vital,
  stats,
}: {
  vital: Vital;
  stats: VitalStats;
}) {
  return (
    <dl className="grid grid-cols-5 gap-px overflow-hidden rounded-xl border border-border-subtle bg-border-subtle">
      {stats.percentiles.map(({ percentile, value }) => (
        <div
          key={percentile}
          className={cn(
            'flex flex-col gap-1 bg-surface px-3 py-2.5',
            percentile === 75 && 'bg-surface-hover',
          )}
        >
          <dt className="text-fg-subtle text-hint">p{percentile}</dt>
          <dd className="text-meta">
            <Rated vital={vital} value={value} />
          </dd>
        </div>
      ))}
    </dl>
  );
}

/**
 * What usually fixes a vital, from web.dev's guides, with the guide to
 * read for the rest.
 */
export function Guidance({ vital }: { vital: Vital }) {
  const tGuide = useTranslations('webVitals.guide');
  const tips = tGuide.raw(vital) as string[];
  return (
    <div className="flex flex-col gap-3">
      <ul className="flex list-disc flex-col gap-1.5 pl-4 text-fg-secondary text-meta">
        {tips.map((tip) => (
          <li key={tip}>{tip}</li>
        ))}
      </ul>
      <a
        href={GUIDES[vital]}
        target="_blank"
        rel="noreferrer"
        className="inline-flex items-center gap-1 self-start text-fg-meta text-meta hover:text-fg"
      >
        {tGuide('more')}
        <ExternalLinkIcon size="sm" />
      </a>
    </div>
  );
}

/** web.dev's guide to each vital. */
const GUIDES: Record<Vital, string> = {
  lcp: 'https://web.dev/articles/optimize-lcp',
  inp: 'https://web.dev/articles/optimize-inp',
  cls: 'https://web.dev/articles/optimize-cls',
  fcp: 'https://web.dev/articles/fcp',
  ttfb: 'https://web.dev/articles/optimize-ttfb',
};

/**
 * One vital on each device or browser: its 75th percentile, how its page
 * loads split, and how many there were. A slow phone or a slow Safari
 * shows here and hides in the total.
 */
export function BreakdownTable({
  vital,
  rows,
  label,
}: {
  vital: Vital;
  rows: WebVitalsBreakdownRow[];
  label: (key: string | null) => string;
}) {
  const tDetail = useTranslations('webVitals.detail');
  if (rows.length === 0) {
    return (
      <p className="px-5 text-fg-subtle text-meta">{tDetail('noSamples')}</p>
    );
  }
  return (
    <ul className="divide-y divide-border-divider border-border-divider border-t">
      {rows.map((row) => (
        <li
          key={row.key ?? ''}
          className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-1.5 px-5 py-2.5"
        >
          <span className="truncate text-fg text-meta">{label(row.key)}</span>
          <span className="flex items-baseline gap-3 text-meta">
            <span className="text-fg-subtle tabular-nums">
              {tDetail('times', { count: row.stats.count })}
            </span>
            <span className="w-16 text-right">
              <Rated vital={vital} value={row.stats.value} />
            </span>
          </span>
          <span className="col-span-2">
            <ShareBar counts={row.stats.distribution} className="h-1.5" />
          </span>
        </li>
      ))}
    </ul>
  );
}

/**
 * Where to start, in one sentence: the page that costs the score the most,
 * its worst vital and, when the SDK named it, the element behind it.
 */
export function FirstFix({
  page,
  vital,
  value,
  cause,
  href,
}: {
  page: string;
  vital: Vital;
  value: number;
  cause?: string;
  href: string;
}) {
  const tFix = useTranslations('webVitals.firstFix');
  return (
    <Link
      href={href}
      className="group flex min-w-0 flex-1 items-center gap-4 px-5 py-4 hover:bg-surface-hover max-lg:rounded-b-2xl lg:rounded-r-2xl"
    >
      <span className="flex min-w-0 flex-1 flex-col gap-1">
        <span className="text-fg-subtle text-hint uppercase tracking-wide">
          {tFix('label')}
        </span>
        <span className="text-fg text-meta">
          {tFix.rich('sentence', {
            vital: VITAL_NAMES[vital].short,
            page,
            mono: (chunks) => <code className="font-mono">{chunks}</code>,
            value: () => <Rated vital={vital} value={value} />,
          })}
          {cause
            ? tFix.rich('cause', {
                cause,
                mono: (chunks) => <code className="font-mono">{chunks}</code>,
              })
            : null}
        </span>
      </span>
      <ChevronRightIcon
        size="md"
        className="shrink-0 text-fg-subtle group-hover:text-fg"
      />
    </Link>
  );
}

/**
 * A vital's trend as CrUX draws one: each bucket a bar split into the
 * share of its page loads that were good, improvable and poor. A bar going
 * green is a vital getting better, whatever happened to traffic.
 */
export function RatingTrendChart({
  vital,
  points,
}: {
  vital: Vital;
  points: WebVitalsPoint[];
}) {
  const t = useTranslations('webVitals');
  const label = useBucketLabel(isDaily(points));
  return (
    <BarsChart
      data={ratingTrend(points)}
      series={[
        { key: 'good', label: t('ratings.good'), color: 'var(--success)' },
        { key: 'meh', label: t('ratings.meh'), color: 'var(--sev-warning)' },
        { key: 'poor', label: t('ratings.poor'), color: 'var(--sev-error)' },
      ]}
      xKey="t"
      stacked
      height={200}
      yDomain={[0, 100]}
      formatX={label}
      formatY={(v) => `${Math.round(v)}%`}
      label={t('trendOf', { vital: VITAL_NAMES[vital].short })}
    />
  );
}

/**
 * The elements a vital was about, the one losing the most score first:
 * the image a slow LCP waited for, the block a layout shift moved, the
 * button an interaction stalled on. What to fix, in order.
 */
export function CausesList({
  vital,
  targets,
}: {
  vital: Vital;
  targets: WebVitalTarget[];
}) {
  const tDetail = useTranslations('webVitals.detail');
  if (targets.length === 0) {
    return (
      <p className="px-5 text-fg-subtle text-meta">
        {vital === 'fcp' || vital === 'ttfb'
          ? tDetail('noTargetsForVital')
          : tDetail('noTargets')}
      </p>
    );
  }
  return (
    <ul className="divide-y divide-border-divider border-border-divider border-t">
      {targets.map((target) => (
        <li
          key={`${target.target} ${target.resource ?? ''}`}
          className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-1.5 px-5 py-2.5"
        >
          <span className="flex min-w-0 flex-col">
            <span
              className="truncate font-mono text-fg text-mono-sm"
              title={target.target}
            >
              {target.target}
            </span>
            {target.resource ? (
              <span
                className="truncate font-mono text-fg-subtle text-hint"
                title={target.resource}
              >
                {target.resource}
              </span>
            ) : null}
          </span>
          <span className="flex items-baseline gap-3 text-meta">
            <span className="text-fg-subtle tabular-nums">
              {tDetail('times', { count: target.count })}
            </span>
            <span className="w-16 text-right">
              <Rated vital={vital} value={target.value} />
            </span>
          </span>
          <span className="col-span-2">
            <ShareBar counts={target.distribution} className="h-1.5" />
          </span>
        </li>
      ))}
    </ul>
  );
}

/** The pages that cost a vital the most, each opening its own page. */
export function WorstPages({
  vital,
  pages,
  href,
}: {
  vital: Vital;
  pages: WebVitalsPage[];
  href: (page: string) => string;
}) {
  const tDetail = useTranslations('webVitals.detail');
  if (pages.length === 0) {
    return (
      <p className="px-5 text-fg-subtle text-meta">{tDetail('noPages')}</p>
    );
  }
  return (
    <ul className="divide-y divide-border-divider border-border-divider border-t">
      {pages.map((page) => {
        const stats = page.vitals.find((v) => v.vital === vital);
        return (
          <li key={page.page} className="relative hover:bg-surface-hover">
            <Link
              href={href(page.page)}
              className="flex items-baseline justify-between gap-4 px-5 py-2.5 after:absolute after:inset-0"
            >
              <span className="truncate font-mono text-fg text-mono-sm">
                {page.page}
              </span>
              <span className="flex shrink-0 items-baseline gap-3 text-meta">
                {stats ? (
                  <>
                    <span className="text-fg-subtle tabular-nums">
                      {tDetail('times', { count: stats.count })}
                    </span>
                    <span className="w-16 text-right">
                      <Rated vital={vital} value={stats.value} />
                    </span>
                  </>
                ) : null}
              </span>
            </Link>
          </li>
        );
      })}
    </ul>
  );
}

/**
 * The worst measurements, each with where it happened: the page, the
 * element, the browser, the device and the network. One with a known page
 * load opens its trace.
 */
export function SamplesList({
  vital,
  samples,
  transactionHref,
}: {
  vital: Vital;
  samples: WebVitalSample[];
  transactionHref: (id: string) => string;
}) {
  const tDetail = useTranslations('webVitals.detail');
  if (samples.length === 0) {
    return (
      <p className="px-5 text-fg-subtle text-meta">{tDetail('noSamples')}</p>
    );
  }
  return (
    <ul className="divide-y divide-border-divider border-border-divider border-t">
      {samples.map((sample) => {
        const where = [
          sample.browser,
          sample.device ? tDetail(`device.${sample.device as 'mobile'}`) : null,
          sample.connection,
          sample.release,
        ].filter(Boolean);
        const body = (
          <>
            <span className="w-16 shrink-0 text-right text-mono-sm">
              <Rated vital={vital} value={sample.value} />
            </span>
            <span className="flex min-w-0 flex-1 flex-col">
              <span className="truncate font-mono text-fg text-mono-sm">
                {sample.page}
                {sample.target ? (
                  <span className="text-fg-subtle"> · {sample.target}</span>
                ) : null}
              </span>
              <span className="truncate text-fg-subtle text-hint">
                {where.join(' · ')}
              </span>
            </span>
            <span className="shrink-0 text-fg-subtle text-meta">
              <RelativeTime at={sample.timestamp} />
            </span>
          </>
        );
        return (
          <li
            key={`${sample.timestamp} ${sample.value} ${sample.page}`}
            className="relative hover:bg-surface-hover"
          >
            {sample.transaction_id ? (
              <Link
                href={transactionHref(sample.transaction_id)}
                className="flex items-center gap-4 px-5 py-2.5 after:absolute after:inset-0"
              >
                {body}
              </Link>
            ) : (
              <div className="flex items-center gap-4 px-5 py-2.5">{body}</div>
            )}
          </li>
        );
      })}
    </ul>
  );
}

const column = createDataTableColumnHelper<WebVitalsPage>();

/** One empty list, so the table's data is stable while the first page loads. */
const NO_PAGES: WebVitalsPage[] = [];

/**
 * The pages, the one that costs the project the most score first. The
 * opportunity carries the one bar, since it is what the ranking is about;
 * then the score and each vital's value.
 */
export function PagesTable({
  page,
  query,
  onQueryChange,
  loading,
  onOpen,
}: {
  /** Absent while the first page loads: the table draws skeleton rows. */
  page?: WebVitalsListPagesResponse;
  query: DataTableQuery;
  onQueryChange: (
    updater: (previous: DataTableQuery) => DataTableQuery,
  ) => void;
  loading: boolean;
  onOpen: (row: WebVitalsPage) => void;
}) {
  const t = useTranslations('webVitals');
  const tVital = useTranslations('transactions.measurements');
  const format = useStableFormatter();

  const items = page?.items ?? NO_PAGES;
  // The bars measure against the page that costs the most, so the ranking
  // reads at a glance however small the shares are.
  const top = Math.max(...items.map((p) => p.opportunity), Number.EPSILON);

  const columns = useMemo(() => {
    const figure = { align: 'end' as const };
    return column.columns([
      column.accessor('page', {
        id: 'page',
        header: t('columns.page'),
        enableHiding: false,
        enableSorting: false,
        cell: ({ getValue }) => (
          <span
            className="truncate font-mono text-fg text-mono"
            title={getValue()}
          >
            {getValue()}
          </span>
        ),
        meta: { label: t('columns.page') },
      }),
      column.accessor('opportunity', {
        id: 'opportunity',
        header: t('columns.opportunity'),
        enableSorting: false,
        cell: ({ getValue }) => (
          <span className="flex w-full items-center gap-2">
            <span className="h-1.5 min-w-0 flex-1 overflow-hidden rounded-pill bg-surface-chip">
              <span
                className="block h-full rounded-pill bg-chart-1"
                style={{
                  width: `${Math.max((getValue() / top) * 100, 1.5)}%`,
                }}
              />
            </span>
            <span className="w-11 shrink-0 text-right text-fg-secondary text-meta tabular-nums">
              {format.number(getValue() * 100, { maximumFractionDigits: 1 })}
            </span>
          </span>
        ),
        meta: { label: t('columns.opportunity'), width: 168 },
      }),
      column.accessor('score', {
        id: 'score',
        header: t('columns.score'),
        enableSorting: false,
        cell: ({ getValue }) => {
          const score = getValue();
          if (score == null) return <span className="text-fg-ghost">–</span>;
          return (
            <span className={cn('tabular-nums', BAND_TEXT[scoreBand(score)])}>
              {outOf100(score)}
            </span>
          );
        },
        meta: {
          label: t('columns.score'),
          width: 104,
          mobile: true,
          ...figure,
        },
      }),
      column.accessor('pageloads', {
        id: 'pageloads',
        header: t('columns.pageloads'),
        enableSorting: false,
        cell: ({ getValue }) => (
          <span className="text-fg-secondary tabular-nums">
            {format.number(getValue(), 'compact')}
          </span>
        ),
        meta: { label: t('columns.pageloads'), width: 96, ...figure },
      }),
      ...VITALS.map((vital) =>
        column.display({
          id: vital,
          header: tVital(vital),
          cell: ({ row }) => {
            const stats = row.original.vitals.find((v) => v.vital === vital);
            return stats ? (
              <Rated vital={vital} value={stats.value} />
            ) : (
              <span className="text-fg-ghost">–</span>
            );
          },
          meta: { label: tVital(vital), width: 100, ...figure },
        }),
      ),
    ]);
  }, [t, tVital, format, top]);

  const table = useDataTable({
    data: items,
    columns,
    rowCount: page?.total_count ?? 0,
    query,
    onQueryChange,
    getRowId: (row) => row.page,
    storageKey: 'web-vital-pages',
  });

  return (
    <DataTable
      table={table}
      loading={!page || loading}
      className="min-h-0 flex-1"
      onRowClick={(row) => onOpen(row.original)}
      empty={{ title: t('emptyWindow') }}
    />
  );
}
