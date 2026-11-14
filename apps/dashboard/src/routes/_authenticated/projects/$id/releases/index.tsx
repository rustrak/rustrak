import type { ProjectResponse, ReleasesListResponse } from '@rustrak/client';
import { type FilterVariants, Page, PageHeader, Skeleton } from '@rustrak/ui';
import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, Navigate } from '@tanstack/react-router';
import { useDeferredValue } from 'react';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { releaseQueries } from '@/features/release/api/queries';
import { ReleasesTable } from '@/features/release/ui/components/releases-table';
import { savedViewQueries } from '@/features/saved-view/api/queries';
import { SavedViews } from '@/features/saved-view/ui/components/saved-views';
import { useEntryView } from '@/features/saved-view/ui/hooks/use-entry-view';
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

const PAGE_SIZE = 25;

/** `environment:` is the one key the bar takes; the rest is the version. */
const VARIANTS: FilterVariants = { environment: 'options' };

interface ReleasesSearch extends TableSearch {
  period?: PeriodChoice;
}

const table = tableSearch({
  variants: VARIANTS,
  pageSize: PAGE_SIZE,
  // No column sorts.
  normalise: (query) => ({ ...query, sorting: [] }),
});

/** The page of releases a URL shows, the same entry for loader and page. */
function releasesList(projectId: number, search: ReleasesSearch) {
  const query = table.read(search);
  const period = search.period ?? DEFAULT_PERIOD;
  return releaseQueries.list(projectId, {
    environment: filterValue(query, 'environment'),
    query: query.search || undefined,
    period: apiPeriod(period),
    page: query.pagination.pageIndex + 1,
    per_page: query.pagination.pageSize,
  });
}

/** The address as typed search; a saved view's text goes through it too. */
function validateReleasesSearch(
  search: Record<string, unknown>,
): ReleasesSearch {
  return {
    ...table.validate(search),
    period: searchOneOf(search.period, PERIOD_CHOICES),
  };
}

export const Route = createFileRoute('/_authenticated/projects/$id/releases/')({
  validateSearch: validateReleasesSearch,
  loaderDeps: ({ search }) => search,
  loader: ({ context: { queryClient }, params, deps }) => {
    const projectId = params.id;
    void queryClient.query(releasesList(projectId, deps)).catch(noop);
    void queryClient.query(projectQueries.environments(projectId)).catch(noop);
    void queryClient
      .query(savedViewQueries.list(projectId, 'releases'))
      .catch(noop);
  },
  pendingComponent: ReleasesLoading,
  component: ReleasesPage,
});

/**
 * The list follows the address a beat behind: a new search or page keeps the
 * rows on screen, dimmed, until the next page is in.
 */
function ReleasesPage() {
  const { id: projectId } = Route.useParams();
  const live = Route.useSearch();
  const search = useDeferredValue(live);
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  const { data: page } = useSuspenseQuery(releasesList(projectId, search));
  const entry = useEntryView(
    projectId,
    'releases',
    live,
    validateReleasesSearch,
  );
  if (entry) {
    return (
      <Navigate
        to="/projects/$id/releases"
        params={{ id: projectId }}
        search={entry}
        replace
      />
    );
  }

  // A stale or shared `?page=` past the end lands on the last page, not on
  // an empty table under a positive count. Only once the rows on screen are
  // the address's: an older answer must not overrule what was since asked.
  const last = pageBeyondEnd(
    table.read(search).pagination.pageIndex + 1,
    page.total_pages,
  );
  if (search === live && last !== false) {
    return (
      <Navigate
        to="/projects/$id/releases"
        params={{ id: projectId }}
        search={{
          ...search,
          page: last,
        }}
        replace
      />
    );
  }

  return (
    <ReleasesScreen project={project} page={page} loading={search !== live} />
  );
}

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
  const { id: projectId } = Route.useParams();
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const query = table.read(search);
  const period = search.period ?? DEFAULT_PERIOD;

  // What the bar can suggest; not the list, so the list does not wait for
  // it, and no suggestions while it loads or when it fails.
  // The skeleton reads nothing: an observer that unmounts mid-flight cancels
  // the loader's request, and the page would ask again.
  const environments =
    useQuery({
      ...projectQueries.environments(projectId),
      enabled: !!project,
      subscribed: !!project,
    }).data ?? [];

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
        actions={
          project ? (
            <SavedViews
              projectId={projectId}
              page="releases"
              search={search}
              onApply={(next) =>
                navigate({ search: validateReleasesSearch(next) })
              }
            />
          ) : undefined
        }
      />

      {page && page.total_count === 0 && !filtered && !loading ? (
        <EmptyState
          title={tReleases('empty.title')}
          description={tReleases.rich('empty.description', {
            code: (chunks) => <code className="font-mono">{chunks}</code>,
          })}
        />
      ) : (
        <Page scroll={false}>
          <ReleasesTable
            projectId={projectId}
            page={page}
            query={query}
            loading={loading}
            environments={environments}
            onQueryChange={(updater) =>
              navigate({
                search: (prev) => ({ ...prev, ...table.apply(updater, prev) }),
              })
            }
            scope={{
              environment: filterValue(query, 'environment'),
              period: search.period,
            }}
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
