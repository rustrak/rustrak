import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { ProjectHeader } from '@/features/project/ui/components/project-header';
import {
  type OverviewPeriod,
  parseOverviewPeriod,
} from '@/features/release/model/session-health';
import { translator } from '@/shared/i18n/intl';
import { searchString } from '@/shared/lib/search-params';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import { OverviewPeriodFilter } from './-components/overview-period-filter';
import { prefetchOverviewTiles } from './-components/overview-queries';
import {
  CounterTiles,
  CrashFreeTile,
  ErrorVolumeTile,
  PerformanceTile,
  SessionHealthTile,
  TopIssuesTile,
} from './-components/overview-tiles';

export const Route = createFileRoute('/_authenticated/projects/$id/')({
  validateSearch: (
    search: Record<string, unknown>,
  ): {
    period?: OverviewPeriod;
  } => ({
    // An unrecognized window would otherwise reach the API, which ignores what
    // it cannot parse and answers with all-time data while no filter button
    // reads as selected. Drop it instead, so the URL and the UI always agree.
    period: parseOverviewPeriod(searchString(search.period)),
  }),
  loaderDeps: ({ search }) => ({ period: search.period }),
  loader: ({ params: { id }, deps: { period }, context: { queryClient } }) => {
    prefetchOverviewTiles(queryClient, { projectId: id, period });
    return queryClient.ensureQueryData(projectQueries.detail(id));
  },
  head: ({ loaderData }) => {
    const t = translator('projectPages');

    if (!loaderData?.success) {
      return { meta: [{ title: t('projectNotFound') }] };
    }

    return {
      meta: [
        { title: t('projectTitle', { project: loaderData.data.name }) },
        {
          name: 'description',
          content: t('overview.meta.description', {
            project: loaderData.data.name,
          }),
        },
      ],
    };
  },
  component: ProjectPage,
});

function ProjectPage() {
  const t = useTranslations('projectPages');
  const { id: projectId } = Route.useParams();
  const { period } = Route.useSearch();
  const { data: projectResult } = useSuspenseQuery(
    projectQueries.detail(projectId),
  );

  if (!projectResult.success) {
    return (
      <LoadFailure error={projectResult.error} title={t('loadProjectFailed')} />
    );
  }

  const project = projectResult.data;
  const tile = { projectId, period };

  return (
    <div className="flex h-[calc(100vh-64px)] flex-col overflow-auto">
      <div className="w-full shrink-0 border-b px-4 py-4 md:px-8 md:py-6">
        <ProjectHeader project={project} />
      </div>

      {/*
        Bento grid: tile size is the hierarchy. The error-volume chart is the
        one thing worth looking at first, so it takes four times the area of a
        counter tile; everything else orbits it. One uniform gap throughout.
      */}
      <div className="flex w-full flex-1 flex-col gap-4 px-4 py-4 md:px-8 md:py-6">
        <OverviewPeriodFilter projectId={projectId} activePeriod={period} />

        {/*
          min-w-0 on every cell that holds a chart: a grid item defaults to
          min-width:auto, so a ResponsiveContainer's measured width can pin its
          column open and the whole page ends up scrolling sideways on a phone.
        */}
        <div className="grid gap-4 xl:grid-cols-4">
          <div className="min-w-0 xl:col-span-2">
            <ErrorVolumeTile {...tile} />
          </div>

          <div className="grid min-w-0 gap-4 sm:grid-cols-2 xl:col-span-2">
            {/* Both counters come from one query, so they share a read. */}
            <CounterTiles {...tile} />
            <div className="min-w-0 sm:col-span-2">
              <CrashFreeTile {...tile} />
            </div>
          </div>

          <div className="min-w-0 xl:col-span-3">
            <SessionHealthTile {...tile} />
          </div>

          <div className="min-w-0">
            <PerformanceTile {...tile} />
          </div>

          <div className="min-w-0 xl:col-span-4">
            <TopIssuesTile {...tile} />
          </div>
        </div>
      </div>
    </div>
  );
}
