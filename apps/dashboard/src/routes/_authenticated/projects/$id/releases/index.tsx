import type { ProjectResponse, ReleasesListResponse } from '@rustrak/client';
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
import { createFileRoute, Navigate } from '@tanstack/react-router';
import { useDeferredValue } from 'react';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { releaseQueries } from '@/features/release/api/queries';
import {
  DEFAULT_PERIOD,
  PERIOD_CHOICES,
  type PeriodChoice,
} from '@/features/release/model/session-health';
import { PeriodFilter } from '@/features/release/ui/components/period-filter';
import { ReleasesTable } from '@/features/release/ui/components/releases-table';
import { searchOneOf, searchString } from '@/shared/lib/search-params';
import { PageLoading } from '@/shared/ui/components/page-loading';

const PAGE_SIZE = 25;
/** `list_releases`'s own cap on `per_page`. */
const MAX_PAGE_SIZE = 100;

/** `environment:` is the one key the bar takes; the rest is the version. */
const VARIANTS: FilterVariants = { environment: 'options' };

/** The search as the table's codecs write it: `q` is the query bar. */
interface ReleasesSearch {
  q?: string;
  period?: PeriodChoice;
  page?: string;
  per?: string;
}

function tableQuery(search: ReleasesSearch): DataTableQuery {
  const params = new URLSearchParams();
  for (const key of ['q', 'page', 'per'] as const) {
    const value = search[key];
    if (value) params.set(key, value);
  }
  const query = parseTableQuery(params, VARIANTS, PAGE_SIZE);
  // The API pages at most 100 releases; a larger `?per=` would have the
  // table expect rows it never receives, and no next page to reach them.
  query.pagination.pageSize = Math.min(
    query.pagination.pageSize,
    MAX_PAGE_SIZE,
  );
  return query;
}

/** The environment the bar picked, if any. */
function environmentOf(query: DataTableQuery): string | undefined {
  const value = query.filters.find((f) => f.id === 'environment')?.value;
  return Array.isArray(value) ? value[0] : undefined;
}

/** The page of releases a URL shows, the same entry for loader and page. */
function releasesList(projectId: number, search: ReleasesSearch) {
  const query = tableQuery(search);
  const period = search.period ?? DEFAULT_PERIOD;
  return releaseQueries.list(projectId, {
    environment: environmentOf(query),
    query: query.search || undefined,
    period: period === 'all' ? undefined : period,
    page: query.pagination.pageIndex + 1,
    per_page: query.pagination.pageSize,
  });
}

export const Route = createFileRoute('/_authenticated/projects/$id/releases/')({
  validateSearch: (search: Record<string, unknown>): ReleasesSearch => ({
    q: searchString(search.q),
    period: searchOneOf(search.period, PERIOD_CHOICES),
    page: searchString(String(search.page ?? '')) || undefined,
    per: searchString(String(search.per ?? '')) || undefined,
  }),
  loaderDeps: ({ search }) => search,
  loader: ({ context: { queryClient }, params, deps }) => {
    const projectId = Number.parseInt(params.id, 10);
    void queryClient.query(projectQueries.detail(projectId)).catch(noop);
    void queryClient.query(releasesList(projectId, deps)).catch(noop);
    void queryClient.query(projectQueries.environments(projectId)).catch(noop);
  },
  pendingComponent: ReleasesLoading,
  component: ReleasesPage,
});

/**
 * The list follows the address a beat behind: a new search or page keeps the
 * rows on screen, dimmed, until the next page is in.
 */
function ReleasesPage() {
  const { id } = Route.useParams();
  const live = Route.useSearch();
  const search = useDeferredValue(live);
  const projectId = Number.parseInt(id, 10);
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  const { data: page } = useSuspenseQuery(releasesList(projectId, search));

  // A stale or shared `?page=` past the end lands on the last page, not on
  // an empty table under a positive count. Only once the rows on screen are
  // the address's: an older answer must not overrule what was since asked.
  const { total_pages } = page;
  if (
    search === live &&
    tableQuery(search).pagination.pageIndex + 1 > Math.max(total_pages, 1)
  ) {
    return (
      <Navigate
        to="/projects/$id/releases"
        params={{ id }}
        search={{
          ...search,
          page: total_pages > 1 ? String(total_pages) : undefined,
        }}
        replace
      />
    );
  }

  return (
    <ReleasesScreen project={project} page={page} loading={search !== live} />
  );
}

/** The same screen, with the project's name and the rows still to come. */
function ReleasesLoading() {
  return (
    <PageLoading>
      <ReleasesScreen loading />
    </PageLoading>
  );
}

function ReleasesScreen({
  project,
  page,
  loading,
}: {
  project?: ProjectResponse;
  page?: ReleasesListResponse;
  loading: boolean;
}) {
  const t = useTranslations('projectPages');
  const tReleases = useTranslations('releases');
  const { id } = Route.useParams();
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const projectId = Number.parseInt(id, 10);
  const query = tableQuery(search);
  const period = search.period ?? DEFAULT_PERIOD;

  // What the bar can suggest; not the list, so the list does not wait for
  // it, and no suggestions while it loads or when it fails.
  const environments =
    useQuery(projectQueries.environments(projectId)).data ?? [];

  const filtered = query.search !== '' || query.filters.length > 0;

  return (
    <div className="flex h-full flex-col">
      {project ? (
        <title>{t('releases.meta.title', { project: project.name })}</title>
      ) : null}
      <PageHeader
        title={t('releases.title')}
        summary={
          project && page
            ? [project.name, tReleases('count', { count: page.total_count })]
            : [<Skeleton key="summary" className="h-4 w-48" />]
        }
      />

      {page && page.total_count === 0 && !filtered && !loading ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 px-page-gutter py-16 text-center">
          <EmptyIcon size="xl" className="mb-2 text-fg-ghost" />
          <Text variant="body">{tReleases('empty.title')}</Text>
          <Text variant="meta" tone="subtle" className="max-w-md">
            {tReleases.rich('empty.description', {
              code: (chunks) => <code className="font-mono">{chunks}</code>,
            })}
          </Text>
        </div>
      ) : (
        <Page scroll={false}>
          <ReleasesTable
            projectId={projectId}
            page={page}
            query={query}
            loading={loading}
            environments={environments}
            onQueryChange={(updater) => {
              const params = serializeTableQuery(
                updater(query),
                VARIANTS,
                PAGE_SIZE,
              );
              navigate({
                search: (prev) => ({
                  ...prev,
                  q: params.get('q') ?? undefined,
                  page: params.get('page') ?? undefined,
                  per: params.get('per') ?? undefined,
                }),
              });
            }}
            scope={{ environment: environmentOf(query), period: search.period }}
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
