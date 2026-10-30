import { MuteIcon, Skeleton, Text } from '@rustrak/ui';
import {
  keepPreviousData,
  noop,
  useQuery,
  useSuspenseQuery,
} from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useDeferredValue } from 'react';
import { useTranslations } from 'use-intl';
import { monitorQueries } from '@/features/monitor/api/queries';
import {
  TIMELINE_PERIODS,
  type TimelinePeriod,
} from '@/features/monitor/lib/timeline';
import { CheckInsTable } from '@/features/monitor/ui/components/check-ins-table';
import { CronSetupGuide } from '@/features/monitor/ui/components/cron-setup-guide';
import { MonitorActions } from '@/features/monitor/ui/components/monitor-actions';
import {
  DurationChart,
  MonitorFacts,
  MonitorFigures,
  MonitorTimeline,
} from '@/features/monitor/ui/components/monitor-detail';
import { MonitorStatus } from '@/features/monitor/ui/components/monitor-status';
import { TimelinePeriodFilter } from '@/features/monitor/ui/components/timeline-period';
import { projectQueries } from '@/features/project/api/queries';
import { searchOneOf, searchPage } from '@/shared/lib/search-params';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { Panel, RailBlock } from '@/shared/ui/components/panel';
import { QueryState } from '@/shared/ui/components/query-state';
import { ScheduleText } from '@/shared/ui/components/schedule-text';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';

const PER_PAGE = 25;
const DEFAULT_PERIOD: TimelinePeriod = '24h';

/**
 * One monitor, laid out as the endpoint and release pages are: what it is
 * and what can be done to it in the header, how its runs went in the reading
 * column -- figures, the strips, durations, every check-in -- and its
 * configuration in the rail.
 */
export const Route = createFileRoute(
  '/_authenticated/projects/$id/crons/m/$slug/',
)({
  validateSearch: (search: Record<string, unknown>) => ({
    page: searchPage(search.page),
    period: searchOneOf(search.period, TIMELINE_PERIODS),
  }),
  loaderDeps: ({ search }) => ({ page: search.page, period: search.period }),
  // The monitor and the page of check-ins are what the page suspends on;
  // the figures and the charts, all drawn from the stats, arrive after.
  loader: ({ context: { queryClient }, params, deps }) => {
    const reads = monitorReads(params, deps);
    void queryClient.query(projectQueries.detail(reads.projectId)).catch(noop);
    void queryClient.query(reads.monitor).catch(noop);
    void queryClient.query(reads.checkIns).catch(noop);
    void queryClient.query(reads.stats).catch(noop);
  },
  pendingComponent: MonitorLoading,
  component: MonitorPage,
});

/** The page's reads for a URL, built once for the loader and the component. */
function monitorReads(
  params: { id: string; slug: string },
  search: { page?: number; period?: TimelinePeriod },
) {
  const projectId = Number.parseInt(params.id, 10);
  return {
    projectId,
    monitor: monitorQueries.detail(projectId, params.slug),
    checkIns: monitorQueries.checkIns(projectId, params.slug, {
      page: search.page ?? 1,
      per_page: PER_PAGE,
    }),
    stats: monitorQueries.stats(projectId, {
      slug: params.slug,
      period: search.period ?? DEFAULT_PERIOD,
    }),
  };
}

/**
 * The check-ins follow the address a beat behind: the next page keeps the
 * one on screen, dimmed, until it is in. Controls read the live address.
 */
function MonitorPage() {
  const t = useTranslations('crons');
  const params = Route.useParams();
  const live = Route.useSearch();
  const search = useDeferredValue(live);
  const navigate = Route.useNavigate();
  const projectId = Number.parseInt(params.id, 10);
  const period = live.period ?? DEFAULT_PERIOD;
  const reads = monitorReads(params, search);
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  const { data: monitor } = useSuspenseQuery(reads.monitor);
  const { data: checkIns } = useSuspenseQuery(reads.checkIns);
  // Another window keeps the one on screen until it arrives.
  const stats = useQuery({ ...reads.stats, placeholderData: keepPreviousData });
  usePageTrail([{ label: monitor.name }]);

  const waiting = monitor.environments.length === 0;
  const loadFailed = t('loadMonitorFailed');

  return (
    <div className="flex h-full flex-col">
      <title>{t('meta.detailTitle', { monitor: monitor.name })}</title>
      <header className="@container shrink-0 border-border-divider border-b px-page-gutter pt-6 pb-5">
        <div className="flex flex-col gap-4 @4xl:flex-row @4xl:items-start @4xl:justify-between @4xl:gap-8">
          <div className="flex min-w-0 flex-col gap-1.5">
            <div className="flex min-w-0 flex-wrap items-center gap-2.5">
              <h1 className="truncate text-fg text-page-title">
                {monitor.name}
              </h1>
              <MonitorStatus monitor={monitor} />
            </div>
            <p className="flex min-w-0 flex-wrap items-center gap-x-2 text-fg-subtle text-meta">
              <span className="font-mono text-mono-sm">{monitor.slug}</span>
              <span aria-hidden="true">·</span>
              <span className="text-fg-secondary">
                <ScheduleText
                  schedule={monitor.config.schedule}
                  timezone={monitor.config.timezone}
                />
              </span>
              {monitor.config.schedule.type === 'crontab' ? (
                <span className="font-mono text-mono-sm">
                  {monitor.config.schedule.value}
                </span>
              ) : null}
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {waiting ? null : (
              <TimelinePeriodFilter
                period={period}
                onChange={(next) =>
                  navigate({
                    search: (prev) => ({
                      ...prev,
                      period: next === DEFAULT_PERIOD ? undefined : next,
                    }),
                    replace: true,
                  })
                }
              />
            )}
            <MonitorActions projectId={projectId} monitor={monitor} />
          </div>
        </div>
      </header>

      <div className="flex min-h-0 flex-1 max-lg:flex-col max-lg:overflow-y-auto">
        <main className="min-w-0 flex-1 lg:overflow-y-auto">
          <div className="flex flex-col gap-5 px-page-gutter pt-5 pb-8">
            {monitor.is_muted ? (
              <p className="flex items-center gap-2 rounded-lg border border-border bg-surface px-4 py-3 text-fg-secondary text-meta">
                <MuteIcon size="sm" className="shrink-0" aria-hidden="true" />
                {t('detail.mutedBanner')}
              </p>
            ) : null}

            {waiting ? (
              <Panel
                title={t('detail.waitingTitle')}
                hint={t('detail.waitingDescription')}
              >
                <CronSetupGuide dsn={project.dsn} slug={monitor.slug} />
              </Panel>
            ) : (
              <>
                <QueryState query={stats} placeholder={null} title={loadFailed}>
                  {(loaded) => (
                    <MonitorFigures monitor={monitor} stats={loaded} />
                  )}
                </QueryState>
                <Panel
                  title={t('detail.timeline')}
                  hint={t('detail.timelineHint')}
                >
                  <QueryState
                    query={stats}
                    placeholder={null}
                    title={loadFailed}
                  >
                    {(loaded) => (
                      <MonitorTimeline monitor={monitor} stats={loaded} />
                    )}
                  </QueryState>
                </Panel>
                <Panel
                  title={t('detail.duration')}
                  hint={t('detail.durationHint')}
                >
                  <QueryState
                    query={stats}
                    placeholder={null}
                    title={loadFailed}
                  >
                    {(loaded) => <DurationChart stats={loaded} />}
                  </QueryState>
                </Panel>
              </>
            )}

            <Panel
              flush
              title={t('detail.checkIns')}
              hint={t('detail.checkInsHint')}
            >
              <CheckInsTable
                page={checkIns}
                currentPage={search.page ?? 1}
                showEnvironment={monitor.environments.length > 1}
                pending={search !== live}
                onPageChange={(next) =>
                  navigate({
                    search: (prev) => ({
                      ...prev,
                      page: next === 1 ? undefined : next,
                    }),
                  })
                }
              />
            </Panel>

            {waiting ? null : (
              <Panel
                title={t('detail.instrument')}
                hint={t('detail.instrumentHint')}
              >
                <CronSetupGuide dsn={project.dsn} slug={monitor.slug} />
              </Panel>
            )}
          </div>
        </main>
        <aside className="border-border-divider bg-panel max-lg:border-t lg:w-80 lg:shrink-0 lg:overflow-y-auto lg:border-l xl:w-88">
          <RailBlock title={t('detail.config')}>
            <MonitorFacts monitor={monitor} />
          </RailBlock>
          <RailBlock title={t('detail.environments')}>
            {waiting ? (
              <Text variant="meta" tone="subtle">
                {t('health.waiting')}
              </Text>
            ) : (
              <ul className="flex flex-col gap-2 text-meta">
                {monitor.environments.map((env) => (
                  <li
                    key={env.id}
                    className="flex items-center justify-between gap-3"
                  >
                    <span className="font-mono text-fg-secondary text-mono-sm">
                      {env.environment}
                    </span>
                    <span
                      className={
                        env.status === 'error'
                          ? 'text-sev-error-fg'
                          : 'text-fg-subtle'
                      }
                    >
                      {t(`environmentStatus.${env.status}`)}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </RailBlock>
        </aside>
      </div>
    </div>
  );
}

/** The monitor's layout, with the monitor and its runs still to come. */
function MonitorLoading() {
  const t = useTranslations('crons');
  const lines = (widths: string[]) => (
    <div className="flex flex-col gap-2.5">
      {widths.map((width, line) => (
        <Skeleton
          // Placeholder lines have no identity beyond their position.
          // react-doctor-disable-next-line react-doctor/no-array-index-as-key
          key={line}
          className={`h-3 ${width}`}
        />
      ))}
    </div>
  );

  return (
    <PageLoading>
      <div className="flex h-full flex-col">
        <header className="@container shrink-0 border-border-divider border-b px-page-gutter pt-6 pb-5">
          <div className="flex flex-col gap-1.5">
            <Skeleton className="h-7 w-56 max-w-full" />
            <Skeleton className="my-0.5 h-3.5 w-72 max-w-full" />
          </div>
        </header>

        <div className="flex min-h-0 flex-1 max-lg:flex-col max-lg:overflow-y-auto">
          <main className="min-w-0 flex-1 lg:overflow-y-auto">
            <div className="flex flex-col gap-5 px-page-gutter pt-5 pb-8">
              <Skeleton className="h-20 w-full rounded-2xl" />
              <Panel
                title={t('detail.timeline')}
                hint={t('detail.timelineHint')}
              >
                <Skeleton className="h-6 w-full" />
              </Panel>
              <Panel
                title={t('detail.checkIns')}
                hint={t('detail.checkInsHint')}
              >
                {lines(['w-full', 'w-full', 'w-full', 'w-2/3'])}
              </Panel>
            </div>
          </main>
          <aside className="border-border-divider bg-panel max-lg:border-t lg:w-80 lg:shrink-0 lg:overflow-y-auto lg:border-l xl:w-88">
            <RailBlock title={t('detail.config')}>
              {lines(['w-full', 'w-full', 'w-2/3'])}
            </RailBlock>
            <RailBlock title={t('detail.environments')}>
              {lines(['w-32'])}
            </RailBlock>
          </aside>
        </div>
      </div>
    </PageLoading>
  );
}
