import type { ReleaseSummary } from '@rustrak/client';
import { cn, formatFilterQuery, Skeleton } from '@rustrak/ui';
import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useDeferredValue } from 'react';
import { useTranslations } from 'use-intl';
import { ISSUE_FILTER_VARIANTS } from '@/features/issue/model/issue-table';
import { projectQueries } from '@/features/project/api/queries';
import { releaseQueries } from '@/features/release/api/queries';
import {
  DEFAULT_PERIOD,
  overviewInterval,
  PERIOD_CHOICES,
  type PeriodChoice,
} from '@/features/release/model/session-health';
import { searchOneOf, searchString } from '@/shared/lib/search-params';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';
import { ReleaseFilters } from './-components/release-filters';
import {
  NewIssuesPanel,
  RailBlock,
  ReleaseDetails,
  ReleaseEnvironments,
  ReleaseHeader,
  ReleaseKpis,
  SessionsPanel,
} from './-components/release-panels';

interface ReleaseSearch {
  environment?: string;
  period?: PeriodChoice;
}

/**
 * The page's reads for a URL, built once for the loader and the components so
 * both name the same cache entries.
 *
 * `version` comes already decoded by the router, which treats an undecodable
 * `%zz` as no match: decoding again would turn a literal `%20` into a space.
 */
function releaseReads(
  projectId: number,
  version: string,
  search: ReleaseSearch,
) {
  const choice = search.period ?? DEFAULT_PERIOD;
  const period = choice === 'all' ? undefined : choice;
  const { environment } = search;
  const interval = overviewInterval(period);
  return {
    choice,
    interval,
    release: releaseQueries.detail(projectId, version, {
      environment,
      period,
    }),
    // The project's own health, the average the release is read against.
    project: releaseQueries.sessionSummary(projectId, { period, environment }),
    series: releaseQueries.sessionTimeseries(projectId, {
      period,
      interval,
      environment,
      release: version,
    }),
    newIssues: releaseQueries.newIssues(projectId, version, { limit: 8 }),
    environments: releaseQueries.healthRows(projectId, version, period),
  };
}

export const Route = createFileRoute(
  '/_authenticated/projects/$id/releases/$release',
)({
  validateSearch: (search: Record<string, unknown>): ReleaseSearch => ({
    environment: searchString(search.environment),
    period: searchOneOf(search.period, PERIOD_CHOICES),
  }),
  loaderDeps: ({ search }) => search,
  // The page waits for the release; the panels around it land on their own.
  loader: ({ context: { queryClient }, params, deps }) => {
    const projectId = Number.parseInt(params.id, 10);
    const reads = releaseReads(projectId, params.release, deps);
    void queryClient.query(reads.release).catch(noop);
    void queryClient.query(reads.project).catch(noop);
    void queryClient.query(reads.series).catch(noop);
    void queryClient.query(reads.newIssues).catch(noop);
    void queryClient.query(reads.environments).catch(noop);
    // The title's project name.
    void queryClient.query(projectQueries.detail(projectId)).catch(noop);
    // The environment filter's choices.
    void queryClient.query(projectQueries.environments(projectId)).catch(noop);
  },
  pendingComponent: ReleaseLoading,
  component: ReleaseDetailPage,
});

/**
 * The page follows its scope a beat behind: a new environment or window
 * keeps the release on screen, dimmed, until its figures for the new scope
 * are in. The filters read the live address.
 */
function ReleaseDetailPage() {
  const tPages = useTranslations('projectPages');
  const params = Route.useParams();
  const live = Route.useSearch();
  const search = useDeferredValue(live);
  const projectId = Number.parseInt(params.id, 10);
  const { data: release } = useSuspenseQuery(
    releaseReads(projectId, params.release, search).release,
  );
  const project = useQuery(projectQueries.detail(projectId)).data;
  usePageTrail([{ label: release.version, mono: true }]);

  return (
    <>
      {project ? (
        <title>
          {tPages('releaseDetail.meta.title', {
            release: release.version,
            project: project.name,
          })}
        </title>
      ) : null}
      <ReleaseScreen
        release={release}
        search={search}
        loading={search !== live}
      />
    </>
  );
}

/** The same page while the release loads; its panels fill in on their own. */
function ReleaseLoading() {
  return (
    <PageLoading>
      <ReleaseScreen search={Route.useSearch()} loading={false} />
    </PageLoading>
  );
}

function ReleaseScreen({
  release,
  search,
  loading,
}: {
  release?: ReleaseSummary;
  /** The scope the figures are read for. */
  search: ReleaseSearch;
  loading: boolean;
}) {
  const t = useTranslations('releases.detail');
  const params = Route.useParams();
  const live = Route.useSearch();
  const navigate = Route.useNavigate();
  const projectId = Number.parseInt(params.id, 10);
  const reads = releaseReads(projectId, params.release, search);
  const { interval } = reads;
  const project = useQuery(reads.project);
  const series = useQuery(reads.series);
  const newIssues = useQuery(reads.newIssues);
  const environments = useQuery(reads.environments);

  const issuesHref = `/projects/${projectId}/issues?${new URLSearchParams({
    view: 'all',
    q: formatFilterQuery(
      [{ id: 'release', value: [params.release] }],
      '',
      ISSUE_FILTER_VARIANTS,
    ),
  })}`;

  return (
    <div className="flex h-full flex-col">
      <ReleaseHeader
        release={release}
        scope={
          <ReleaseFilters
            projectId={projectId}
            scope={{
              environment: live.environment,
              period: live.period ?? DEFAULT_PERIOD,
            }}
            onChange={(change) =>
              navigate({
                search: (prev) => ({
                  ...prev,
                  ...('environment' in change
                    ? { environment: change.environment }
                    : {}),
                  ...(change.period
                    ? {
                        period:
                          change.period === DEFAULT_PERIOD
                            ? undefined
                            : change.period,
                      }
                    : {}),
                }),
                replace: true,
              })
            }
          />
        }
      />

      {/* The issue page's shape: the release's figures and what it broke
          in the reading column, its facts in the rail beside it. */}
      <div
        aria-busy={loading}
        className={cn(
          'flex min-h-0 flex-1 max-lg:flex-col max-lg:overflow-y-auto',
          loading && 'opacity-60',
        )}
      >
        <main className="min-w-0 flex-1 lg:overflow-y-auto">
          <div className="flex flex-col gap-5 px-page-gutter pt-5 pb-8">
            {release ? (
              <ReleaseKpis release={release} project={project} />
            ) : (
              <Skeleton className="h-[7.25rem] rounded-2xl" />
            )}
            <SessionsPanel read={series} intervalHours={interval} />
            <NewIssuesPanel
              projectId={projectId}
              read={newIssues}
              issuesHref={issuesHref}
            />
          </div>
        </main>
        <aside className="border-border-divider bg-panel max-lg:border-t lg:w-80 lg:shrink-0 lg:overflow-y-auto lg:border-l xl:w-88">
          <RailBlock title={t('details')}>
            {release ? (
              <ReleaseDetails release={release} />
            ) : (
              <div className="flex flex-col gap-3">
                <Skeleton className="h-4 w-full" />
                <Skeleton className="h-4 w-full" />
                <Skeleton className="h-4 w-2/3" />
              </div>
            )}
          </RailBlock>
          <RailBlock title={t('environments')}>
            <ReleaseEnvironments read={environments} />
          </RailBlock>
        </aside>
      </div>
    </div>
  );
}
