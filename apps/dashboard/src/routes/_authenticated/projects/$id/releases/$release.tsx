import type { ReleaseSummary } from '@rustrak/client';
import { formatFilterQuery, Skeleton } from '@rustrak/ui';
import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useDeferredValue } from 'react';
import { useTranslations } from 'use-intl';
import { ISSUE_FILTER_VARIANTS } from '@/features/issue/model/issue-table';
import { projectQueries } from '@/features/project/api/queries';
import { releaseQueries } from '@/features/release/api/queries';
import {
  apiPeriod,
  DEFAULT_PERIOD,
  overviewInterval,
  PERIOD_CHOICES,
  type PeriodChoice,
  periodParam,
} from '@/shared/lib/period';
import { searchOneOf, searchString } from '@/shared/lib/search-params';
import {
  DetailBody,
  DetailMain,
  DetailRail,
} from '@/shared/ui/components/detail-layout';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { RailBlock } from '@/shared/ui/components/panel';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';
import { ReleaseFilters } from './-components/release-filters';
import {
  NewIssuesPanel,
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
  const period = apiPeriod(choice);
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
    const projectId = params.id;
    const reads = releaseReads(projectId, params.release, deps);
    void queryClient.query(reads.release).catch(noop);
    void queryClient.query(reads.project).catch(noop);
    void queryClient.query(reads.series).catch(noop);
    void queryClient.query(reads.newIssues).catch(noop);
    void queryClient.query(reads.environments).catch(noop);
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
  const projectId = params.id;
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

/** The same page while the release loads, its panels skeleton. */
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
  const projectId = params.id;
  const reads = releaseReads(projectId, params.release, search);
  const { interval } = reads;
  // The skeleton reads nothing: an observer that unmounts mid-flight cancels
  // the loader's request, and the page would ask again.
  const idle = { enabled: !!release, subscribed: !!release };
  const project = useQuery({ ...reads.project, ...idle });
  const series = useQuery({ ...reads.series, ...idle });
  const newIssues = useQuery({ ...reads.newIssues, ...idle });
  const environments = useQuery({ ...reads.environments, ...idle });

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
            held={!release}
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
                    ? { period: periodParam(change.period) }
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
      <DetailBody busy={loading}>
        <DetailMain>
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
        </DetailMain>
        <DetailRail>
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
        </DetailRail>
      </DetailBody>
    </div>
  );
}
