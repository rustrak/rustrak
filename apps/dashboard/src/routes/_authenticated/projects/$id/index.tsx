import type { ProjectResponse } from '@rustrak/client';
import { Page, PageHeader, Skeleton, Text } from '@rustrak/ui';
import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { PlatformIcon } from 'platformicons';
import { useFormatter, useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import {
  DEFAULT_PERIOD,
  PERIOD_CHOICES,
  type PeriodChoice,
} from '@/features/release/model/session-health';
import { searchOneOf, searchString } from '@/shared/lib/search-params';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { OverviewFilters } from './-components/overview-filters';
import {
  ErrorVolumePanel,
  IssuesPanel,
  KpiRow,
  LatencyPanel,
  SessionsPanel,
} from './-components/overview-panels';
import {
  type OverviewFilters as Filters,
  overviewReads,
} from './-components/overview-reads';

interface OverviewSearch {
  period?: PeriodChoice;
  environment?: string;
  release?: string;
  compare?: true;
}

/** The URL's filters, as the panels read them. */
function overviewFilters(projectId: number, search: OverviewSearch) {
  const choice = search.period ?? DEFAULT_PERIOD;
  const period = choice === 'all' ? undefined : choice;
  const filters: Filters = {
    projectId,
    period,
    environment: search.environment,
    release: search.release,
  };
  // Nothing comes before all time, so there is nothing to compare it with.
  const compare = !!search.compare && period !== undefined;
  return { choice, filters, compare };
}

export const Route = createFileRoute('/_authenticated/projects/$id/')({
  validateSearch: (search: Record<string, unknown>): OverviewSearch => ({
    // An unrecognized value is dropped rather than passed on: the API ignores
    // what it cannot parse and answers unfiltered, while no control reads as
    // selected. So the URL and the screen always agree.
    period: searchOneOf(search.period, PERIOD_CHOICES),
    environment: searchString(search.environment),
    release: searchString(search.release),
    compare: search.compare === true || search.compare === 'true' || undefined,
  }),
  loaderDeps: ({ search }) => search,
  // Every read is started here. The page waits only for the project; each
  // panel fills in when its own read lands, failing on its own.
  loader: ({ context: { queryClient }, params, deps }) => {
    const projectId = Number.parseInt(params.id, 10);
    const { filters, compare } = overviewFilters(projectId, deps);
    const reads = overviewReads(filters, compare);
    void queryClient.query(projectQueries.detail(projectId)).catch(noop);
    void queryClient.query(reads.summary).catch(noop);
    void queryClient.query(reads.events).catch(noop);
    void queryClient.query(reads.sessionSummary).catch(noop);
    void queryClient.query(reads.sessionTrend).catch(noop);
    void queryClient.query(reads.latency).catch(noop);
    void queryClient.query(reads.environments).catch(noop);
    void queryClient.query(reads.releases).catch(noop);
    if (reads.previousEvents)
      void queryClient.query(reads.previousEvents).catch(noop);
    void queryClient.query(reads.issues('recent')).catch(noop);
  },
  pendingComponent: ProjectLoading,
  component: ProjectPage,
});

function ProjectPage() {
  const { id } = Route.useParams();
  const projectId = Number.parseInt(id, 10);
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  return <ProjectScreen project={project} />;
}

/**
 * The same screen while the project loads. The filters and the panels are
 * real: neither waits for the project, so each panel shows its own
 * placeholder and fills in as its read lands, even before the header does.
 */
function ProjectLoading() {
  return (
    <PageLoading>
      <ProjectScreen />
    </PageLoading>
  );
}

/**
 * The panels follow the address directly: none of them suspends, so a new
 * filter swaps each one for its own placeholder until its read is in.
 */
function ProjectScreen({ project }: { project?: ProjectResponse }) {
  const t = useTranslations('projectPages');
  const format = useFormatter();
  const { id } = Route.useParams();
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const projectId = Number.parseInt(id, 10);
  const { choice, filters, compare } = overviewFilters(projectId, search);
  const reads = overviewReads(filters, compare);

  return (
    // On a phone the header scrolls away with the page: held in place, the
    // title and the filters would take a third of the screen.
    <div className="flex h-full flex-col max-lg:overflow-y-auto">
      {project ? (
        <title>{t('projectTitle', { project: project.name })}</title>
      ) : null}
      <PageHeader
        title={
          <span className="flex min-w-0 items-center gap-3">
            {project ? (
              <>
                <PlatformIcon
                  platform={project.platform ?? 'other'}
                  size={26}
                  format="lg"
                  className="shrink-0 rounded-md"
                />
                <span className="truncate">{project.name}</span>
              </>
            ) : (
              <>
                <Skeleton className="size-[26px] shrink-0 rounded-md" />
                <Skeleton className="h-7 w-48" />
              </>
            )}
          </span>
        }
        summary={
          project
            ? [
                <Text key="slug" variant="mono-sm" tone="meta">
                  {project.slug}
                </Text>,
                t('overview.storedEvents', {
                  count: format.number(project.stored_event_count, 'compact'),
                }),
              ]
            : [<Skeleton key="summary" className="h-4 w-56" />]
        }
        actions={
          <OverviewFilters
            projectId={projectId}
            period={choice}
            environment={search.environment}
            release={search.release}
            onChange={(change) =>
              navigate({ search: (prev) => ({ ...prev, ...change }) })
            }
          />
        }
      />

      <Page className="max-lg:flex-none max-lg:overflow-visible">
        <KpiRow reads={reads} filters={filters} />

        {/* min-w-0 on every cell holding a chart: a grid item defaults to
            min-width:auto, so a chart's measured width can pin its column
            open and the page ends up scrolling sideways on a phone. */}
        <ErrorVolumePanel
          reads={reads}
          filters={filters}
          compare={compare}
          onCompareChange={(next) =>
            navigate({
              search: (prev) => ({ ...prev, compare: next || undefined }),
            })
          }
        />

        <div className="grid gap-4 xl:grid-cols-5">
          <div className="min-w-0 xl:col-span-3">
            <IssuesPanel reads={reads} projectId={projectId} />
          </div>
          <div className="min-w-0 xl:col-span-2">
            <SessionsPanel reads={reads} filters={filters} />
          </div>
        </div>

        <LatencyPanel reads={reads} projectId={projectId} />
      </Page>
    </div>
  );
}
