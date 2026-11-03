import type {
  TransactionResponse,
  TransactionStatsResponse,
  TransactionsGetStatGroupTimeseriesResponse,
} from '@rustrak/client';
import {
  BarsChart,
  cn,
  formatSpanDuration,
  Tag,
  TimeSeriesChart,
} from '@rustrak/ui';
import type { ReactNode } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import {
  axisDuration,
  failureTone,
  perMinute,
} from '@/features/transaction/model/endpoints';
import { Link } from '@/shared/ui/components/link';

/*
 * The pieces of an endpoint's page. The page lays them out the way the
 * release page does: the figures that answer "is it OK" and how it moved in
 * the reading column, the facts that describe it in the rail.
 */

const FAILURE_TEXT = {
  warning: 'text-sev-warning-fg',
  bad: 'text-sev-error-fg',
} as const;

/**
 * Four figures in one strip: how much it runs, how slow it gets, how often it
 * fails, and what it costs in all. The rest describe it, and live in the rail.
 */
export function EndpointFigures({
  group,
  windowHours,
}: {
  group: TransactionStatsResponse;
  windowHours?: number;
}) {
  const t = useTranslations('performance.detail');
  const format = useFormatter();
  const tone = failureTone(group.failure_rate);
  const percent = (value: number) =>
    format.number(value, { style: 'percent', maximumFractionDigits: 1 });

  const figures: [string, ReactNode, ReactNode][] = [
    [
      t('callsFigure'),
      format.number(group.count, 'compact'),
      windowHours
        ? t('perMinute', {
            rate: format.number(perMinute(group.count, windowHours), {
              maximumFractionDigits: 2,
            }),
          })
        : null,
    ],
    [
      'p95',
      formatSpanDuration(group.p95_ms),
      t('p50', { time: formatSpanDuration(group.p50_ms) }),
    ],
    [
      t('failures'),
      <span className={tone ? FAILURE_TEXT[tone] : undefined}>
        {percent(group.failure_rate)}
      </span>,
      t('failedCalls', {
        count: format.number(Math.round(group.failure_rate * group.count)),
      }),
    ],
    [
      t('timeSpent'),
      percent(group.time_share),
      t('totalTime', { time: formatSpanDuration(group.total_ms) }),
    ],
  ];

  return (
    <dl className="grid grid-cols-2 gap-px overflow-hidden rounded-2xl border border-border-subtle bg-border-subtle xl:grid-cols-4">
      {figures.map(([label, value, footnote]) => (
        <div
          key={label}
          className="flex min-w-0 flex-col gap-1.5 bg-surface px-4 py-3.5"
        >
          <dt className="truncate text-fg-meta text-meta">{label}</dt>
          <dd className="text-fg text-title tabular-nums">{value}</dd>
          <dd className="truncate text-fg-subtle text-hint">
            {footnote ?? ' '}
          </dd>
        </div>
      ))}
    </dl>
  );
}

/** The endpoint's facts, one per line, as on the release page's rail. */
export function EndpointFacts({
  group,
  op,
  environment,
}: {
  group: TransactionStatsResponse;
  op: string | null;
  environment?: string;
}) {
  const t = useTranslations('performance.detail');
  const format = useFormatter();
  const last = new Date(group.last_seen);
  const facts: [string, ReactNode][] = [
    [t('operation'), op && <span className="font-mono">{op}</span>],
    [t('environment'), environment ?? t('allEnvironments')],
    ['p50', formatSpanDuration(group.p50_ms)],
    ['p95', formatSpanDuration(group.p95_ms)],
    ['p99', formatSpanDuration(group.p99_ms)],
    [t('average'), formatSpanDuration(group.avg_ms)],
    [
      t('lastSeen'),
      <span title={format.dateTime(last, 'dateTime')}>
        {format.relativeTime(last)}
      </span>,
    ],
  ];

  return (
    <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2.5 text-meta">
      {facts
        .filter(([, value]) => value)
        .map(([label, value]) => (
          <div key={label} className="contents">
            <dt className="text-fg-subtle">{label}</dt>
            <dd className="min-w-0 truncate text-right text-fg-secondary tabular-nums">
              {value}
            </dd>
          </div>
        ))}
    </dl>
  );
}

function useBucketLabel(points: TransactionsGetStatGroupTimeseriesResponse) {
  const format = useFormatter();
  const [first, second] = points;
  const daily =
    first && second
      ? new Date(second.bucket).getTime() - new Date(first.bucket).getTime() >=
        86_400_000
      : false;
  return (value: string | number) =>
    format.dateTime(
      new Date(value),
      daily
        ? { month: 'short', day: 'numeric' }
        : { month: 'short', day: 'numeric', hour: 'numeric' },
    );
}

/** p50 and p95 over the window; an empty bucket is a gap, not a zero. */
export function LatencyChart({
  points,
}: {
  points: TransactionsGetStatGroupTimeseriesResponse;
}) {
  const t = useTranslations('performance.detail');
  const label = useBucketLabel(points);
  return (
    <TimeSeriesChart
      data={points.map((p) => ({
        t: new Date(p.bucket).getTime(),
        p50: p.count ? p.p50_ms : null,
        p95: p.count ? p.p95_ms : null,
      }))}
      series={[
        { key: 'p50', label: 'p50', color: 'var(--chart-1)' },
        { key: 'p95', label: 'p95', color: 'var(--chart-2)' },
      ]}
      xKey="t"
      height={220}
      formatX={label}
      formatY={axisDuration}
      label={t('latency')}
    />
  );
}

/** Calls per bucket, the failed ones on top. */
export function CallsChart({
  points,
}: {
  points: TransactionsGetStatGroupTimeseriesResponse;
}) {
  const t = useTranslations('performance.detail');
  const format = useFormatter();
  const label = useBucketLabel(points);
  return (
    <BarsChart
      data={points.map((p) => ({
        t: new Date(p.bucket).getTime(),
        ok: p.count - p.failures,
        failed: p.failures,
      }))}
      series={[
        { key: 'ok', label: t('ok'), color: 'var(--chart-3)' },
        { key: 'failed', label: t('failed'), color: 'var(--sev-error)' },
      ]}
      xKey="t"
      height={220}
      formatX={label}
      formatY={(v) => format.number(v, 'compact')}
      label={t('calls')}
    />
  );
}

/** The slowest samples as a table: how long, where, when, and how it ended. */
export function SlowestSamples({
  projectId,
  rows,
}: {
  projectId: number;
  rows: TransactionResponse[];
}) {
  const t = useTranslations('performance.detail');
  const format = useFormatter();
  if (rows.length === 0) {
    return <p className="px-5 text-fg-subtle text-meta">{t('noSamples')}</p>;
  }
  const longest = Math.max(...rows.map((r) => r.duration_ms ?? 0), 1);

  return (
    <table className="w-full text-meta">
      <thead>
        <tr className="text-left text-column text-fg-meta">
          <th className="px-5 pb-2 font-normal">{t('duration')}</th>
          <th className="hidden pb-2 font-normal md:table-cell">
            {t('environment')}
          </th>
          <th className="hidden pb-2 font-normal md:table-cell">
            {t('release')}
          </th>
          <th className="pb-2 font-normal">{t('status')}</th>
          <th className="px-5 pb-2 text-right font-normal">{t('when')}</th>
        </tr>
      </thead>
      <tbody className="divide-y divide-border-divider border-border-divider border-t">
        {rows.map((row) => {
          const failed = row.status != null && row.status !== 'ok';
          const href = `/projects/${projectId}/performance/${row.id}`;
          return (
            // The whole row opens the trace: the duration's link stretches
            // over it, so a click, the keyboard and a middle click all reach
            // the one link.
            <tr key={row.id} className="relative hover:bg-surface-hover">
              <td className="w-2/5 px-5 py-2.5">
                <Link
                  href={href}
                  className="flex items-center gap-3 after:absolute after:inset-0"
                >
                  <span className="w-16 shrink-0 text-right font-mono text-fg text-mono-sm tabular-nums">
                    {row.duration_ms == null
                      ? '–'
                      : formatSpanDuration(row.duration_ms)}
                  </span>
                  <span className="h-1.5 min-w-0 flex-1 overflow-hidden rounded-pill bg-surface-chip">
                    <span
                      className={cn(
                        'block h-full rounded-pill',
                        failed ? 'bg-sev-error' : 'bg-chart-2',
                      )}
                      style={{
                        width: `${((row.duration_ms ?? 0) / longest) * 100}%`,
                      }}
                    />
                  </span>
                </Link>
              </td>
              <td className="hidden py-2.5 text-fg-secondary md:table-cell">
                {row.environment || '–'}
              </td>
              <td className="hidden py-2.5 font-mono text-fg-secondary text-mono-sm md:table-cell">
                {row.release || '–'}
              </td>
              <td className="py-2.5">
                {failed ? (
                  <Tag variant="soft" tone="error">
                    {row.status}
                  </Tag>
                ) : (
                  <span className="text-fg-subtle">ok</span>
                )}
              </td>
              <td className="px-5 py-2.5 text-right text-fg-subtle">
                {format.relativeTime(new Date(row.timestamp))}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
