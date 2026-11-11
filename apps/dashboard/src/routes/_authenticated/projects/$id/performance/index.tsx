import type {
  ProjectResponse,
  TransactionsGetStatsData,
  TransactionsGetStatsResponse,
} from '@rustrak/client';
import { type FilterVariants, Page, PageHeader, Skeleton } from '@rustrak/ui';
import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, Navigate } from '@tanstack/react-router';
import { useDeferredValue } from 'react';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { transactionQueries } from '@/features/transaction/api/queries';
import { endpointHref } from '@/features/transaction/model/endpoints';
import { EndpointsTable } from '@/features/transaction/ui/components/endpoints-table';
import {
  apiPeriod,
  DEFAULT_PERIOD,
  PERIOD_CHOICES,
  type PeriodChoice,
  periodParam,
} from '@/shared/lib/period';
import { searchOneOf } from '@/shared/lib/search-params';
import {
  filterValue,
  pageBeyondEnd,
  type TableSearch,
  tableSearch,
} from '@/shared/lib/table-search';
import { EmptyState } from '@/shared/ui/components/empty-state';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { PeriodFilter } from '@/shared/ui/components/period-filter';
import { useRouter } from '@/shared/ui/hooks/use-router';

const PAGE_SIZE = 25;

/** `environment:` is the one key the bar takes; the rest is the name. */
const VARIANTS: FilterVariants = { environment: 'options' };

type StatsSort = NonNullable<
  NonNullable<TransactionsGetStatsData['query']>['sort']
>;
const SORTS: StatsSort[] = ['time_spent', 'count', 'avg', 'failure_rate'];

interface PerformanceSearch extends TableSearch {
  period?: PeriodChoice;
}

const table = tableSearch({
  variants: VARIANTS,
  pageSize: PAGE_SIZE,
  // The API ranks by one known column, always from the most down, and reads
  // one environment: a typed `environment:a,b` keeps its first, so the bar
  // never names one the table is not showing.
  normalise: (query) => ({
    ...query,
    sorting: query.sorting
      .filter((s) => SORTS.includes(s.id as StatsSort))
      .slice(0, 1)
      .map((s) => ({ ...s, desc: true })),
    filters: query.filters.map((filter) =>
      filter.id === 'environment' && Array.isArray(filter.value)
        ? { ...filter, value: filter.value.slice(0, 1) }
        : filter,
    ),
  }),
  // The ranking the page is about, shown as the sort it is.
  defaultSort: { id: 'time_spent', desc: true },
});

function statsParams(
  search: PerformanceSearch,
): NonNullable<TransactionsGetStatsData['query']> {
  const query = table.read(search);
  return {
    environment: filterValue(query, 'environment'),
    query: query.search || undefined,
    period: apiPeriod(search.period ?? DEFAULT_PERIOD),
    sort: query.sorting[0]?.id as StatsSort,
    page: query.pagination.pageIndex + 1,
    per_page: query.pagination.pageSize,
  };
}

export const Route = createFileRoute(
  '/_authenticated/projects/$id/performance/',
)({
  validateSearch: (search: Record<string, unknown>): PerformanceSearch => ({
    ...table.validate(search),
    period: searchOneOf(search.period, PERIOD_CHOICES),
  }),
  loaderDeps: ({ search }) => search,
  loader: ({ context: { queryClient }, params, deps }) => {
    const projectId = params.id;
    void queryClient
      .query(transactionQueries.stats(projectId, statsParams(deps)))
      .catch(noop);
    void queryClient.query(projectQueries.environments(projectId)).catch(noop);
  },
  pendingComponent: PerformanceLoading,
  component: PerformancePage,
});

/**
 * The page of endpoints and the project are what the page waits for. The
 * page follows the address a beat behind: a new filter, window or page keeps
 * the rows on screen, dimmed, until the next page is in.
 */
function PerformancePage() {
  const { id: projectId } = Route.useParams();
  const live = Route.useSearch();
  const search = useDeferredValue(live);
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  const { data: page } = useSuspenseQuery(
    transactionQueries.stats(projectId, statsParams(search)),
  );
  const last = pageBeyondEnd(
    table.read(search).pagination.pageIndex + 1,
    page.total_pages,
  );
  if (search === live && last !== false) {
    return (
      <Navigate
        to="/projects/$id/performance"
        params={{ id: projectId }}
        search={{ ...search, page: last }}
        replace
      />
    );
  }
  return (
    <PerformanceScreen
      project={project}
      page={page}
      loading={search !== live}
    />
  );
}

function PerformanceLoading() {
  return (
    <PageLoading>
      <PerformanceScreen loading={false} />
    </PageLoading>
  );
}

function PerformanceScreen({
  project,
  page,
  loading,
}: {
  project?: ProjectResponse;
  page?: TransactionsGetStatsResponse;
  loading: boolean;
}) {
  const t = useTranslations('performance');
  const tPages = useTranslations('projectPages');
  const { id: projectId } = Route.useParams();
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const query = table.read(search);
  const period = search.period ?? DEFAULT_PERIOD;
  const environment = filterValue(query, 'environment');

  // What the bar can suggest: none while they load or when they fail.
  // The skeleton reads nothing: an observer that unmounts mid-flight cancels
  // the loader's request, and the page would ask again.
  const environments =
    useQuery({
      ...projectQueries.environments(projectId),
      enabled: !!project,
      subscribed: !!project,
    }).data ?? [];

  const router = useRouter();

  const filtered = query.search !== '' || query.filters.length > 0;

  return (
    <div className="flex h-full flex-col">
      {project ? (
        <title>
          {tPages('performance.meta.title', { project: project.name })}
        </title>
      ) : null}
      <PageHeader
        title={tPages('performance.title')}
        summary={
          project && page
            ? [project.name, t('count', { count: page.total_count })]
            : [<Skeleton key="summary" className="h-4 w-56" />]
        }
      />

      {page?.total_count === 0 && !filtered && period === 'all' ? (
        <NoTransactions />
      ) : (
        <Page scroll={false}>
          <EndpointsTable
            page={page}
            query={query}
            loading={loading}
            environments={environments}
            onOpen={(row) =>
              router.push(
                endpointHref(
                  projectId,
                  { name: row.transaction_name, op: row.op },
                  // The page's own period, `all` included: the endpoint page
                  // reads a missing one as the default, not as all time.
                  {
                    period: search.period,
                    environment,
                  },
                ),
              )
            }
            onQueryChange={(updater) =>
              navigate({
                search: (prev) => ({
                  ...prev,
                  ...table.write(updater(query)),
                }),
              })
            }
            period={
              <PeriodFilter
                period={period}
                onChange={(next) =>
                  navigate({
                    search: (prev) => ({
                      ...prev,
                      period: periodParam(next),
                      page: undefined,
                    }),
                  })
                }
              />
            }
          />
        </Page>
      )}
    </div>
  );
}

/** Before any transaction arrives: what to turn on in the SDK. */
function NoTransactions() {
  const t = useTranslations('performance');
  return (
    <EmptyState
      title={t('empty.title')}
      description={t.rich('empty.description', {
        code: (chunks) => <code className="font-mono">{chunks}</code>,
      })}
    />
  );
}
