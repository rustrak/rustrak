import {
  BarsChart,
  Button,
  CompareIcon,
  cn,
  SegmentedControl,
  SegmentedItem,
  Sparkline,
  TimeSeriesChart,
} from '@rustrak/ui';
import { useQuery } from '@tanstack/react-query';
import { type ReactNode, useState } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import { IssueRow } from '@/features/issue/ui/components/issue-row';
import { withPreviousPeriod } from '@/features/project/lib/overview-series';
import { overviewInterval } from '@/shared/lib/period';
import { Link } from '@/shared/ui/components/link';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import { MetricDeltaText } from '@/shared/ui/components/metric-delta';
import { Panel, PanelPlaceholder } from '@/shared/ui/components/panel';
import { QueryState } from '@/shared/ui/components/query-state';
import { useBucketLabel } from '@/shared/ui/hooks/use-bucket-label';
import type {
  IssueView,
  OverviewFilters,
  OverviewReads,
} from './overview-reads';
import { TransactionP95Bars } from './transaction-p95-bars';

/*
 * The overview's panels. Each one subscribes to its own reads and renders its
 * own placeholder and its own failure, so the page fills in as each read
 * lands rather than at the speed of the slowest. A read two panels share --
 * the event series feeds the error KPI's sparkline and the volume chart -- is
 * one cache entry, fetched once.
 *
 * A failed read renders as a failure, never as an empty series: a flat line
 * at zero reads as "nothing is breaking", the most dangerous thing this page
 * could say while wrong.
 */
/** One headline figure, how it moved, and its shape over the window. */
function Kpi({
  label,
  value,
  delta,
  footnote,
  trend,
}: {
  label: string;
  value: ReactNode;
  delta?: ReactNode;
  footnote?: ReactNode;
  trend?: ReactNode;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-2 rounded-2xl border border-border-subtle bg-surface px-4 pt-3.5 pb-4 sm:px-5 sm:pt-4 sm:pb-4.5">
      <div className="flex items-center justify-between gap-2">
        <span className="truncate text-meta text-fg-meta">{label}</span>
        {delta}
      </div>
      <div className="flex items-end justify-between gap-4">
        <span className="text-page-title text-fg tabular-nums">{value}</span>
        {trend}
      </div>
      {/* A line is kept even when empty, so the cards in a row line up. */}
      <p className="truncate text-hint text-fg-subtle">{footnote ?? ' '}</p>
    </div>
  );
}

function KpiPlaceholder() {
  return (
    <div className="h-[7.25rem] animate-pulse rounded-2xl border border-border-subtle bg-surface motion-reduce:animate-none" />
  );
}

/**
 * No sessions at all: two cards of dashes would weigh as much as the figures
 * beside them. One quiet note holds both places instead.
 */
function NoSessionsNote() {
  const t = useTranslations('projectPages.overview');
  return (
    <div className="col-span-2 flex min-w-0 flex-col justify-center gap-1 rounded-2xl border border-dashed border-border-subtle px-4 py-3.5 sm:px-5">
      <span className="truncate text-meta text-fg-meta">
        {t('crashFreeSessions')} · {t('crashFreeUsers')}
      </span>
      <span className="text-value text-fg-subtle">{t('noSessions')}</span>
    </div>
  );
}

export function KpiRow({
  reads,
  filters,
}: {
  reads: OverviewReads;
  filters: OverviewFilters;
}) {
  const t = useTranslations('projectPages.overview');
  const format = useFormatter();
  const summary = useQuery(reads.summary);
  const events = useQuery(reads.events);
  const health = useQuery(reads.sessionSummary);
  const trend = useQuery(reads.sessionTrend);
  const percent = (rate: number | null | undefined) =>
    rate == null
      ? '–'
      : format.number(rate, { style: 'percent', maximumFractionDigits: 2 });

  // The sparkline is decoration on the error card: without its series the
  // card still says what it counts.
  const eventTrend = events.data?.map((p) => p.total) ?? null;

  const failure = [summary, health, trend].find(
    (read) => read.data === undefined && read.isError,
  )?.error;
  if (failure) {
    return (
      <LoadFailure
        error={failure.error}
        title={t('kpisFailed')}
        notFoundOnMissing={false}
      />
    );
  }

  return (
    <div className="grid grid-cols-2 gap-3 sm:gap-4 xl:grid-cols-4">
      {summary.data ? (
        <>
          <Kpi
            label={t('errors')}
            value={format.number(summary.data.events.current, 'compact')}
            delta={<MetricDeltaText metric={summary.data.events} />}
            footnote={
              [filters.environment, filters.release]
                .filter(Boolean)
                .join(' · ') || null
            }
            trend={
              eventTrend ? (
                <Sparkline
                  values={eventTrend}
                  tone="danger"
                  label={t('errors')}
                  className="hidden h-8 w-28 shrink-0 sm:block"
                />
              ) : null
            }
          />
          <Kpi
            label={t('newIssues')}
            value={format.number(summary.data.new_issues.current)}
            delta={<MetricDeltaText metric={summary.data.new_issues} />}
            footnote={t('openIssues', {
              count: format.number(summary.data.open_issues),
            })}
          />
        </>
      ) : (
        <>
          <KpiPlaceholder />
          <KpiPlaceholder />
        </>
      )}

      {health.data?.total === 0 ? (
        <NoSessionsNote />
      ) : health.data && trend.data ? (
        <>
          <Kpi
            label={t('crashFreeSessions')}
            value={percent(health.data.crash_free_sessions_rate)}
            footnote={t('sessionsCount', {
              count: format.number(health.data.total, 'compact'),
            })}
            trend={
              <Sparkline
                values={trend.data.map((p) => p.crashed)}
                tone="danger"
                label={t('crashes')}
                className="hidden h-8 w-28 shrink-0 sm:block"
              />
            }
          />
          <Kpi
            label={t('crashFreeUsers')}
            value={percent(health.data.crash_free_users_rate)}
            footnote={t('activeReleases', {
              count: health.data.active_releases,
            })}
          />
        </>
      ) : (
        <>
          <KpiPlaceholder />
          <KpiPlaceholder />
        </>
      )}
    </div>
  );
}

export function ErrorVolumePanel({
  reads,
  filters,
  compare,
  onCompareChange,
}: {
  reads: OverviewReads;
  filters: OverviewFilters;
  compare: boolean;
  onCompareChange: (compare: boolean) => void;
}) {
  const t = useTranslations('projectPages.overview');
  const format = useFormatter();
  const interval = overviewInterval(filters.period);
  const label = useBucketLabel(interval >= 24);
  const events = useQuery(reads.events);
  // Not asked for unless comparing: without the window before, the
  // observer stands on the current series and stays disabled.
  const previous = useQuery({
    ...(reads.previousEvents ?? reads.events),
    enabled: reads.previousEvents !== null,
  });
  // A comparison that did not load costs the dashed line, not the chart.
  const comparison = reads.previousEvents ? previous.data : undefined;
  const total = events.data?.reduce((sum, p) => sum + p.total, 0) ?? null;

  return (
    <Panel
      title={t('errorVolume')}
      hint={
        total === null
          ? ' '
          : t('eventsInWindow', { count: format.number(total) })
      }
      actions={
        // Lives with the chart it changes: it overlays this chart and no
        // other, so in the page header it would read as a page filter.
        <Button
          variant="ghost"
          size="sm"
          icon={CompareIcon}
          selected={compare}
          // All time has no period before it to compare with.
          disabled={!filters.period}
          onClick={() => onCompareChange(!compare)}
        >
          {t('compare')}
        </Button>
      }
    >
      <QueryState
        query={events}
        placeholder={<PanelPlaceholder height={280} />}
        title={t('errorVolume')}
      >
        {(points) =>
          compare ? (
            <TimeSeriesChart
              data={(comparison
                ? withPreviousPeriod(points, comparison)
                : points.map((p) => ({ ...p, previous: undefined }))
              ).map((p) => ({
                t: new Date(p.bucket).getTime(),
                total: p.total,
                previous: p.previous,
              }))}
              series={[
                {
                  key: 'total',
                  label: t('thisPeriod'),
                  color: 'var(--sev-error)',
                },
                // No comparison means no line, rather than a line of zeros
                // that reads as a quiet period.
                ...(comparison
                  ? [
                      {
                        key: 'previous',
                        label: t('previousPeriod'),
                        color: 'var(--chart-muted)',
                      },
                    ]
                  : []),
              ]}
              xKey="t"
              integer
              height={280}
              formatX={label}
              formatY={(v) => format.number(v, 'compact')}
              label={t('errorVolume')}
            />
          ) : (
            <BarsChart
              data={points.map((p) => ({
                t: new Date(p.bucket).getTime(),
                // Fatal reads as an error here; the issue says which.
                error: p.fatal + p.error,
                warning: p.warning,
                info: p.info,
              }))}
              series={[
                {
                  key: 'error',
                  label: t('levelError'),
                  color: 'var(--sev-error)',
                },
                {
                  key: 'warning',
                  label: t('levelWarning'),
                  color: 'var(--sev-warning)',
                },
                {
                  key: 'info',
                  label: t('levelInfo'),
                  color: 'var(--sev-info)',
                },
              ]}
              xKey="t"
              integer
              height={280}
              formatX={label}
              formatY={(v) => format.number(v, 'compact')}
              label={t('errorVolume')}
            />
          )
        }
      </QueryState>
    </Panel>
  );
}

/**
 * The open issues to look at: the ones firing now, the newest, or the
 * loudest. In an environment, only the issues seen there.
 */
export function IssuesPanel({
  reads,
  projectId,
}: {
  reads: OverviewReads;
  projectId: number;
}) {
  const t = useTranslations('projectPages.overview');
  const [view, setView] = useState<IssueView>('recent');
  const issues = useQuery(reads.issues(view));

  return (
    <Panel
      title={t('issues')}
      hint={t('issuesDescription')}
      actions={
        <SegmentedControl
          aria-label={t('issues')}
          value={view}
          onValueChange={(value) => setView(value as IssueView)}
        >
          <SegmentedItem value="recent">{t('issuesRecent')}</SegmentedItem>
          <SegmentedItem value="new">{t('issuesNew')}</SegmentedItem>
          <SegmentedItem value="frequent">{t('issuesFrequent')}</SegmentedItem>
        </SegmentedControl>
      }
      footer={
        <Link
          href={`/projects/${projectId}/issues`}
          className="text-meta text-fg-subtle hover:text-fg"
        >
          {t('viewAllIssues')}
        </Link>
      }
    >
      <QueryState
        query={issues}
        placeholder={<PanelPlaceholder height={288} />}
        title={t('issues')}
      >
        {(page) =>
          page.items.length === 0 ? (
            <p className="flex h-72 items-center justify-center text-meta text-fg-subtle">
              {t('noIssuesYet')}
            </p>
          ) : (
            <ul className="-mx-5 -mt-1">
              {page.items.map((issue) => (
                <IssueRow key={issue.id} projectId={projectId} issue={issue} />
              ))}
            </ul>
          )
        }
      </QueryState>
    </Panel>
  );
}

export function SessionsPanel({
  reads,
  filters,
}: {
  reads: OverviewReads;
  filters: OverviewFilters;
}) {
  const t = useTranslations('projectPages.overview');
  const format = useFormatter();
  const label = useBucketLabel(overviewInterval(filters.period) >= 24);
  const health = useQuery(reads.sessionSummary);
  const trend = useQuery(reads.sessionTrend);
  const placeholder = <PanelPlaceholder height={288} />;

  return (
    <Panel title={t('sessionHealth')} hint={t('sessionHealthSubtitle')}>
      <QueryState
        query={health}
        placeholder={placeholder}
        title={t('sessionHealth')}
      >
        {(health) => (
          <QueryState
            query={trend}
            placeholder={placeholder}
            title={t('sessionHealth')}
          >
            {(series) =>
              health.total === 0 ? (
                <div className="flex h-72 flex-col items-center justify-center gap-1.5 text-center">
                  <p className="text-value font-medium text-fg">
                    {t('noSessions')}
                  </p>
                  <p className="max-w-xs text-meta text-fg-subtle">
                    {t('noSessionsHint')}
                  </p>
                </div>
              ) : (
                <div className="flex flex-col gap-4">
                  <dl className="grid grid-cols-3 gap-3">
                    <Figure
                      label={t('sessions')}
                      value={format.number(health.total, 'compact')}
                    />
                    <Figure
                      label={t('errored')}
                      value={format.number(health.errored, 'compact')}
                      tone="text-sev-warning-fg"
                    />
                    <Figure
                      label={t('crashed')}
                      value={format.number(health.crashed, 'compact')}
                      tone="text-sev-error-fg"
                    />
                  </dl>
                  <TimeSeriesChart
                    data={series.map((p) => ({
                      t: new Date(p.bucket).getTime(),
                      rate:
                        p.crash_free_sessions_rate == null
                          ? null
                          : p.crash_free_sessions_rate * 100,
                    }))}
                    series={[
                      {
                        key: 'rate',
                        label: t('crashFreeSessions'),
                        color: 'var(--chart-3)',
                      },
                    ]}
                    xKey="t"
                    height={208}
                    formatX={label}
                    formatY={(v) =>
                      format.number(v / 100, {
                        style: 'percent',
                        maximumFractionDigits: 1,
                      })
                    }
                    label={t('crashFreeSessions')}
                  />
                </div>
              )
            }
          </QueryState>
        )}
      </QueryState>
    </Panel>
  );
}

function Figure({
  label,
  value,
  tone = 'text-fg',
}: {
  label: string;
  value: string;
  tone?: string;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <dt className="truncate text-hint text-fg-meta">{label}</dt>
      <dd className={cn('text-section tabular-nums', tone)}>{value}</dd>
    </div>
  );
}

export function LatencyPanel({
  reads,
  projectId,
}: {
  reads: OverviewReads;
  projectId: number;
}) {
  const t = useTranslations('projectPages.overview');
  const stats = useQuery(reads.latency);

  return (
    <Panel title={t('latency')} hint={t('latencySubtitle')}>
      <QueryState
        query={stats}
        placeholder={<PanelPlaceholder height={250} />}
        title={t('latency')}
      >
        {(stats) => (
          <TransactionP95Bars projectId={projectId} rows={stats.items} />
        )}
      </QueryState>
    </Panel>
  );
}
