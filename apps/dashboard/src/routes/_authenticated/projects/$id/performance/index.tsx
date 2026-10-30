import type {
  ProjectResponse,
  TransactionsGetStatsData,
  TransactionsGetStatsResponse,
} from '@rustrak/client';
import {
  type DataTableQuery,
  EmptyIcon,
  type FilterVariants,
  Page,
  PageHeader,
  parseTableQuery,
  Skeleton,
  serializeTableQuery,
  Text,
} from '@rustrak/ui';
import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useDeferredValue } from 'react';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import {
  DEFAULT_PERIOD,
  PERIOD_CHOICES,
  type PeriodChoice,
} from '@/features/release/model/session-health';
import { PeriodFilter } from '@/features/release/ui/components/period-filter';
import { transactionQueries } from '@/features/transaction/api/queries';
import { endpointHref } from '@/features/transaction/model/endpoints';
import { EndpointsTable } from '@/features/transaction/ui/components/endpoints-table';
import { searchOneOf, searchString } from '@/shared/lib/search-params';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { useRouter } from '@/shared/ui/hooks/use-router';

const PAGE_SIZE = 25;

/** `environment:` is the one key the bar takes; the rest is the name. */
const VARIANTS: FilterVariants = { environment: 'options' };

type StatsSort = NonNullable<
  NonNullable<TransactionsGetStatsData['query']>['sort']
>;
const SORTS: StatsSort[] = ['time_spent', 'count', 'avg', 'failure_rate'];

/** The search as the table's codecs write it, and the window. */
interface PerformanceSearch {
  q?: string;
  sort?: string;
  page?: string;
  per?: string;
  period?: PeriodChoice;
}

function tableQuery(search: PerformanceSearch): DataTableQuery {
  const params = new URLSearchParams();
  for (const key of ['q', 'sort', 'page', 'per'] as const) {
    const value = search[key];
    if (value) params.set(key, value);
  }
  const query = parseTableQuery(params, VARIANTS, PAGE_SIZE);
  // The API reads one environment: a typed `environment:a,b` keeps its first,
  // so the bar never names one the table is not showing.
  for (const filter of query.filters) {
    if (filter.id === 'environment' && Array.isArray(filter.value)) {
      filter.value = filter.value.slice(0, 1);
    }
  }
  // The ranking the page is about, shown as the sort it is.
  if (query.sorting.length === 0) {
    query.sorting = [{ id: 'time_spent', desc: true }];
  }
  return query;
}

function statsParams(
  search: PerformanceSearch,
): NonNullable<TransactionsGetStatsData['query']> {
  const query = tableQuery(search);
  const period = search.period ?? DEFAULT_PERIOD;
  const environment = query.filters.find((f) => f.id === 'environment')?.value;
  const sort = query.sorting[0]?.id as StatsSort | undefined;
  return {
    environment: Array.isArray(environment)
      ? (environment[0] ?? undefined)
      : undefined,
    query: query.search || undefined,
    period: period === 'all' ? undefined : period,
    sort: sort && SORTS.includes(sort) ? sort : undefined,
    page: query.pagination.pageIndex + 1,
    per_page: query.pagination.pageSize,
  };
}

/** The address of a table query: every ranking from the most down, and the
 * default one left out. */
function searchFor(
  updated: DataTableQuery,
): Pick<PerformanceSearch, 'q' | 'sort' | 'page' | 'per'> {
  const next = {
    ...updated,
    sorting: updated.sorting.map((s) => ({ ...s, desc: true })),
  };
  const params = serializeTableQuery(next, VARIANTS, PAGE_SIZE);
  const ranking = next.sorting[0]?.id;
  return {
    q: params.get('q') ?? undefined,
    sort:
      ranking === 'time_spent' ? undefined : (params.get('sort') ?? undefined),
    page: params.get('page') ?? undefined,
    per: params.get('per') ?? undefined,
  };
}

export const Route = createFileRoute(
  '/_authenticated/projects/$id/performance/',
)({
  validateSearch: (search: Record<string, unknown>): PerformanceSearch => ({
    q: searchString(search.q),
    sort: searchString(search.sort),
    page: searchString(String(search.page ?? '')) || undefined,
    per: searchString(String(search.per ?? '')) || undefined,
    period: searchOneOf(search.period, PERIOD_CHOICES),
  }),
  loaderDeps: ({ search }) => search,
  loader: ({ context: { queryClient }, params, deps }) => {
    const projectId = Number.parseInt(params.id, 10);
    void queryClient.query(projectQueries.detail(projectId)).catch(noop);
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
  const { id } = Route.useParams();
  const projectId = Number.parseInt(id, 10);
  const live = Route.useSearch();
  const search = useDeferredValue(live);
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  const { data: page } = useSuspenseQuery(
    transactionQueries.stats(projectId, statsParams(search)),
  );
  return (
    <PerformanceScreen
      project={project}
      page={page}
      loading={search !== live}
    />
  );
}

/** The same screen, with the project's facts and the rows still to come. */
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
  const { id } = Route.useParams();
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const projectId = Number.parseInt(id, 10);
  const query = tableQuery(search);
  const period = search.period ?? DEFAULT_PERIOD;
  const { environment } = statsParams(search);
  const scope = {
    period: period === 'all' ? undefined : period,
    environment: environment ?? undefined,
  };

  // What the bar can suggest: none while they load or when they fail.
  const environments =
    useQuery(projectQueries.environments(projectId)).data ?? [];

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
                    environment: scope.environment,
                  },
                ),
              )
            }
            onQueryChange={(updater) =>
              navigate({
                search: (prev) => ({
                  ...prev,
                  ...searchFor(updater(query)),
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
                      period: next === DEFAULT_PERIOD ? undefined : next,
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
    <div className="flex flex-1 flex-col items-center justify-center gap-2 px-page-gutter py-16 text-center">
      <EmptyIcon size="xl" className="mb-2 text-fg-ghost" />
      <Text variant="body">{t('empty.title')}</Text>
      <Text variant="meta" tone="subtle" className="max-w-md">
        {t.rich('empty.description', {
          code: (chunks) => <code className="font-mono">{chunks}</code>,
        })}
      </Text>
    </div>
  );
}
