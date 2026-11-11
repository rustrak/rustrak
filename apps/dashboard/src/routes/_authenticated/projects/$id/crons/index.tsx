import type {
  MonitorEnvironmentStats,
  MonitorResponse,
  ProjectResponse,
} from '@rustrak/client';
import {
  Button,
  NewIcon,
  Page,
  PageHeader,
  QueryBar,
  type QueryField,
  Skeleton,
} from '@rustrak/ui';
import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useDeferredValue } from 'react';
import { useTranslations } from 'use-intl';
import { monitorQueries } from '@/features/monitor/api/queries';
import { newMonitorHref } from '@/features/monitor/lib/href';
import {
  filterMonitors,
  MONITOR_STATES,
  monitorHealth,
} from '@/features/monitor/lib/status';
import {
  TIMELINE_PERIODS,
  type TimelinePeriod,
} from '@/features/monitor/lib/timeline';
import { CronsEmpty } from '@/features/monitor/ui/components/crons-empty';
import { MonitorList } from '@/features/monitor/ui/components/monitor-list';
import { TimelinePeriodFilter } from '@/features/monitor/ui/components/timeline-period';
import { projectQueries } from '@/features/project/api/queries';
import { searchOneOf, searchString } from '@/shared/lib/search-params';
import { Link } from '@/shared/ui/components/link';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { usePermissionsLater } from '@/shared/ui/hooks/use-permissions';

const DEFAULT_PERIOD: TimelinePeriod = '24h';

/**
 * Crons: every monitor of the project and how its runs went, Sentry's crons
 * overview. A row per monitor, a strip per environment, every strip on one
 * time axis so a shared failure reads straight down the list.
 */
export const Route = createFileRoute('/_authenticated/projects/$id/crons/')({
  validateSearch: (search: Record<string, unknown>) => ({
    q: searchString(search.q),
    state: searchOneOf(search.state, MONITOR_STATES),
    period: searchOneOf(search.period, TIMELINE_PERIODS),
  }),
  loaderDeps: ({ search }) => ({ period: search.period }),
  // The stats are the strips every row is drawn with, so they are the page
  // as much as the list is.
  loader: ({ context: { queryClient }, params, deps }) => {
    const projectId = params.id;
    void queryClient.query(monitorQueries.list(projectId)).catch(noop);
    void queryClient
      .query(
        monitorQueries.stats(projectId, {
          period: deps.period ?? DEFAULT_PERIOD,
        }),
      )
      .catch(noop);
  },
  pendingComponent: CronsLoading,
  component: CronsPage,
});

/** How often an empty page asks again, so the first check-in shows itself. */
const EMPTY_POLL_MS = 5_000;

/**
 * The list follows the address a beat behind: a new search, state or window
 * keeps the rows on screen, dimmed, until the next strips are in.
 */
function CronsPage() {
  const { id: projectId } = Route.useParams();
  const live = Route.useSearch();
  const search = useDeferredValue(live);
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  // The snippet creates the monitor on its first run, so an empty list asks
  // again until one appears and the list replaces the setup screen.
  const { data: monitors } = useSuspenseQuery({
    ...monitorQueries.list(projectId),
    refetchInterval: (query) =>
      query.state.data?.length === 0 ? EMPTY_POLL_MS : false,
  });
  // Asks again with the list, or the monitor the first check-in created
  // would show "waiting" next to the strip that check-in drew.
  const { data: stats } = useSuspenseQuery({
    ...monitorQueries.stats(projectId, {
      period: search.period ?? DEFAULT_PERIOD,
    }),
    refetchInterval: (query) =>
      query.state.data?.length === 0 ? EMPTY_POLL_MS : false,
  });
  const shown = filterMonitors(monitors, search.q ?? '', search.state);

  return (
    <CronsScreen
      project={project}
      monitors={monitors}
      shown={shown}
      stats={stats}
      loading={search !== live}
    />
  );
}

function CronsLoading() {
  return (
    <PageLoading>
      <CronsScreen loading />
    </PageLoading>
  );
}

function CronsScreen({
  project,
  monitors,
  shown,
  stats,
  loading,
}: {
  project?: ProjectResponse;
  /** Every monitor, for the counts; `shown` is what the filters keep. */
  monitors?: MonitorResponse[];
  shown?: MonitorResponse[];
  stats?: MonitorEnvironmentStats[];
  loading: boolean;
}) {
  const t = useTranslations('crons');
  const { id: projectId } = Route.useParams();
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const period = search.period ?? DEFAULT_PERIOD;
  const newHref = usePermissionsLater().can('monitors', 'create', projectId)
    ? newMonitorHref(projectId)
    : undefined;
  const failing =
    monitors?.filter((m) => monitorHealth(m) === 'failing').length ?? 0;
  const fields: QueryField[] = [
    {
      key: 'state',
      label: t('state'),
      variant: 'options',
      multiple: false,
      options: MONITOR_STATES.map((value) => ({
        value,
        label: t(`health.${value}`),
      })),
    },
  ];

  return (
    <div className="flex h-full flex-col">
      {project ? (
        <title>{t('meta.listTitle', { project: project.name })}</title>
      ) : null}
      <PageHeader
        title={t('title')}
        summary={
          project && monitors
            ? [
                project.name,
                t('count', { count: monitors.length }),
                failing > 0 ? (
                  <span className="text-sev-error-fg">
                    {t('failingCount', { count: failing })}
                  </span>
                ) : null,
              ]
            : [<Skeleton key="summary" className="h-4 w-56" />]
        }
        actions={
          newHref && monitors && monitors.length > 0 ? (
            <Button
              variant="primary"
              icon={NewIcon}
              render={<Link href={newHref} />}
            >
              {t('newMonitor')}
            </Button>
          ) : null
        }
      />

      {project && monitors?.length === 0 ? (
        <Page>
          <CronsEmpty dsn={project.dsn} newHref={newHref} />
        </Page>
      ) : (
        <Page scroll={false}>
          {/* Narrow, the bar takes a row and the window the next. */}
          <div className="flex flex-wrap items-center gap-2">
            <QueryBar
              className="min-w-0 max-sm:basis-full"
              fields={fields}
              filters={
                search.state ? [{ id: 'state', value: [search.state] }] : []
              }
              search={search.q ?? ''}
              placeholder={t('search')}
              onChange={(next) => {
                const state = next.filters.find((f) => f.id === 'state')?.value;
                navigate({
                  search: (prev) => ({
                    ...prev,
                    q: next.search.trim() || undefined,
                    state: Array.isArray(state)
                      ? searchOneOf(state[0], MONITOR_STATES)
                      : undefined,
                  }),
                  replace: true,
                });
              }}
            />
            <TimelinePeriodFilter
              period={period}
              onChange={(next) =>
                navigate({
                  search: (prev) => ({
                    ...prev,
                    period: next === DEFAULT_PERIOD ? undefined : next,
                  }),
                })
              }
            />
          </div>
          <MonitorList
            projectId={projectId}
            monitors={shown}
            stats={stats}
            loading={loading}
          />
        </Page>
      )}
    </div>
  );
}
