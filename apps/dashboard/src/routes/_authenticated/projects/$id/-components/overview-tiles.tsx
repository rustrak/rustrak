import type { RustrakError } from '@rustrak/client';
import { useQuery } from '@tanstack/react-query';
import { useFormatter, useTranslations } from 'use-intl';
import { IssueListCard } from '@/features/issue/ui/components/issue-list-card';
import { CrashFreeTrend } from '@/features/release/ui/components/crash-free-trend';
import { SessionHealthArea } from '@/features/release/ui/components/session-health-area';
import { combine } from '@/shared/lib/results';
import { ErrorVolumeChart } from '@/shared/ui/components/error-volume-chart';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/shared/ui/components/shadcn/card';
import { Skeleton } from '@/shared/ui/components/shadcn/skeleton';
import { StatTile } from '@/shared/ui/components/stat-tile';
import { TransactionP95Bars } from '@/shared/ui/components/transaction-p95-bars';
import { type TileProps, tileQueries } from './overview-queries';

/**
 * Every tile takes the same props so the grid can fill in independently: each
 * one owns its own fetch and its own skeleton, rather than the page blocking
 * on the slowest query before anything paints.
 *
 * The skeleton moved *into* the tile when the Suspense boundaries went. Under
 * Next the page named a fallback height per tile; here the tile that knows how
 * tall it renders is the one that says how tall its placeholder is, which is
 * one fewer pair of numbers to keep in step.
 */
function TileShell({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle?: string;
  children: React.ReactNode;
}) {
  return (
    <Card size="sm" className="h-full">
      <CardHeader>
        <CardTitle className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
          {title}
        </CardTitle>
        {subtitle ? (
          <CardDescription className="text-xs">{subtitle}</CardDescription>
        ) : null}
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  );
}

/** Placeholder that holds the tile's footprint while its query resolves. */
export function TileSkeleton({ height = 140 }: { height?: number }) {
  return (
    <Card size="sm" className="h-full">
      <CardHeader>
        <Skeleton className="h-3 w-28" />
      </CardHeader>
      <CardContent>
        <Skeleton className="w-full" style={{ height }} />
      </CardContent>
    </Card>
  );
}

/**
 * Every tile below renders {@link TileFailure} instead of its chart when its
 * fetch fails. A tile that fell back to an empty series would draw a flat line
 * at zero, which reads as "nothing is breaking" -- the most dangerous thing
 * this page could say while wrong.
 */
function TileFailure({ error, title }: { error: RustrakError; title: string }) {
  return (
    <TileShell title={title}>
      {/* 404 stays in place: one tile's endpoint answering 404 is not grounds
          for replacing the whole overview with the app's not-found page. */}
      <LoadFailure error={error} title={title} notFoundOnMissing={false} />
    </TileShell>
  );
}

export function ErrorVolumeTile({ projectId, period }: TileProps) {
  const t = useTranslations('projectPages');
  const { data: timeseries } = useQuery(
    tileQueries.errorVolume({ projectId, period }),
  );

  if (!timeseries) return <TileSkeleton height={300} />;

  if (!timeseries.success) {
    return (
      <TileFailure error={timeseries.error} title={t('overview.errorVolume')} />
    );
  }

  return (
    <TileShell title={t('overview.errorVolume')}>
      <ErrorVolumeChart data={timeseries.data} />
    </TileShell>
  );
}

export function CounterTiles({ projectId, period }: TileProps) {
  const format = useFormatter();
  const t = useTranslations('projectPages');
  const { data: result } = useQuery(
    tileQueries.counters({ projectId, period }),
  );

  if (!result) {
    return (
      <>
        <TileSkeleton height={56} />
        <TileSkeleton height={56} />
      </>
    );
  }
  if (!result.success) {
    // One fetch fills two grid cells, so it has to fail as two. Returning a
    // single `TileFailure` left the grid one child short and the "New issues"
    // tile simply disappeared, with nothing marking that a metric was missing.
    return (
      <>
        <TileFailure error={result.error} title={t('overview.events')} />
        <TileFailure error={result.error} title={t('overview.newIssues')} />
      </>
    );
  }

  const summary = result.data;

  return (
    <>
      <StatTile
        label={t('overview.events')}
        metric={summary.events}
        polarity="up-is-bad"
      />
      <StatTile
        label={t('overview.newIssues')}
        metric={summary.new_issues}
        polarity="up-is-bad"
        footnote={t('overview.openIssues', {
          count: format.number(summary.open_issues),
        })}
      />
    </>
  );
}

export function CrashFreeTile({ projectId, period }: TileProps) {
  const t = useTranslations('projectPages');

  // The headline rates and the shape behind them come from two endpoints, so
  // they are fetched together rather than split across two Suspense
  // boundaries: half the tile arriving before the other half would flash.
  //
  // Either one failing takes the whole tile to its failure state. Rendering the
  // trend beside a missing headline, or a "0 sessions" headline beside a real
  // trend, invents a relationship between two figures only one of which was
  // measured.
  const { data: summaryResult } = useQuery(
    tileQueries.sessionSummary({ projectId, period }),
  );
  const { data: timeseriesResult } = useQuery(
    tileQueries.sessionTimeseries({ projectId, period }),
  );

  if (!summaryResult || !timeseriesResult) {
    return <TileSkeleton height={132} />;
  }
  const loaded = combine([summaryResult, timeseriesResult]);

  if (!loaded.success) {
    return (
      <TileFailure
        error={loaded.error}
        title={t('overview.crashFreeSessions')}
      />
    );
  }

  const [summary, timeseries] = loaded.data;

  return (
    <TileShell title={t('overview.crashFreeSessions')}>
      <CrashFreeTrend
        sessionsRate={summary.crash_free_sessions_rate}
        usersRate={summary.crash_free_users_rate}
        totalSessions={summary.total}
        data={timeseries}
      />
    </TileShell>
  );
}

export function SessionHealthTile({ projectId, period }: TileProps) {
  const t = useTranslations('projectPages');
  // The same read as the crash-free tile's, so it is fetched once for both.
  const { data: timeseries } = useQuery(
    tileQueries.sessionTimeseries({ projectId, period }),
  );

  if (!timeseries) return <TileSkeleton height={250} />;

  if (!timeseries.success) {
    return (
      <TileFailure
        error={timeseries.error}
        title={t('overview.sessionHealth')}
      />
    );
  }

  return (
    <TileShell
      title={t('overview.sessionHealth')}
      subtitle={t('overview.sessionHealthSubtitle')}
    >
      <SessionHealthArea data={timeseries.data} height={220} />
    </TileShell>
  );
}

export function PerformanceTile({ projectId }: TileProps) {
  const t = useTranslations('projectPages');
  const { data: stats } = useQuery(tileQueries.performance({ projectId }));

  if (!stats) return <TileSkeleton height={250} />;

  if (!stats.success) {
    return <TileFailure error={stats.error} title={t('overview.latency')} />;
  }

  return (
    <TileShell
      title={t('overview.latency')}
      subtitle={t('overview.latencySubtitle')}
    >
      <TransactionP95Bars projectId={projectId} rows={stats.data.items} />
    </TileShell>
  );
}

export function TopIssuesTile({ projectId }: TileProps) {
  const t = useTranslations('projectPages');
  const { data: response } = useQuery(tileQueries.topIssues({ projectId }));

  if (!response) return <TileSkeleton height={180} />;

  if (!response.success) {
    return (
      <TileFailure error={response.error} title={t('overview.topIssues')} />
    );
  }

  return (
    <IssueListCard
      projectId={projectId}
      issues={response.data.items}
      title={t('overview.topIssues')}
      subtitle={t('overview.topIssuesSubtitle')}
      emptyMessage={t('overview.noIssuesYet')}
    />
  );
}
