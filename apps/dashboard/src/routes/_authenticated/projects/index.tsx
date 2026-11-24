import type { ProjectsListResponse } from '@rustrak/client';
import {
  Button,
  cn,
  NewIcon,
  Page,
  PageHeader,
  QueryBar,
  type QueryField,
  SelectorIcon,
  Skeleton,
} from '@rustrak/ui';
import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, Navigate } from '@tanstack/react-router';
import type { ColumnFiltersState } from '@tanstack/react-table';
import { type ReactNode, useDeferredValue } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { ProjectsGrid } from '@/features/project/ui/components/projects-grid';
import { savedViewQueries } from '@/features/saved-view/api/queries';
import { SavedViews } from '@/features/saved-view/ui/components/saved-views';
import { useEntryView } from '@/features/saved-view/ui/hooks/use-entry-view';
import {
  searchOneOf,
  searchPage,
  searchString,
} from '@/shared/lib/search-params';
import { Link } from '@/shared/ui/components/link';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { ScopeBar, ScopeSegment } from '@/shared/ui/components/scope-bar';
import { usePermissionsLater } from '@/shared/ui/hooks/use-permissions';

/**
 * Window for the per-row stats.
 *
 * Fixed rather than user-selectable: this page answers "which project should
 * I look at", and `/projects/[id]` already owns choosing a time range.
 */
const STATS_PERIOD = '24h';

/**
 * One choice of the sort control, each a `sort` and `order` the API takes.
 * The first is the API's own default, which the address leaves out.
 */
const SORTS = {
  newest: { sort: 'created', order: 'desc' },
  recent: { sort: 'last_seen', order: 'desc' },
  oldest: { sort: 'created', order: 'asc' },
  name: { sort: 'name', order: 'asc' },
} as const;
type ProjectsSort = keyof typeof SORTS;
const SORT_CHOICES = Object.keys(SORTS) as ProjectsSort[];

interface ProjectsSearch {
  page?: number;
  sort?: Exclude<ProjectsSort, 'newest'>;
  query?: string;
}

/** The address as typed search; a saved view's text goes through it too. */
function validateProjectsSearch(
  search: Record<string, unknown>,
): ProjectsSearch {
  const sort = searchOneOf(search.sort, SORT_CHOICES);
  return {
    page: searchPage(search.page),
    sort: sort === 'newest' ? undefined : sort,
    query: searchString(search.query),
  };
}

export const Route = createFileRoute('/_authenticated/projects/')({
  validateSearch: validateProjectsSearch,
  loaderDeps: ({ search }) => search,
  // One request, stats included: the server aggregates the whole page in two
  // queries. Asking per row would be 20 round trips for a table that renders
  // above the fold.
  loader: ({ context: { queryClient }, deps }) => {
    void queryClient.query(projectsPage(deps)).catch(noop);
    void queryClient
      .query(savedViewQueries.list(undefined, 'projects'))
      .catch(noop);
  },
  pendingComponent: ProjectsLoading,
  component: ProjectsPage,
});

function projectsPage(search: ProjectsSearch) {
  return projectQueries.list({
    page: search.page ?? 1,
    per_page: 20,
    stats_period: STATS_PERIOD,
    ...SORTS[search.sort ?? 'newest'],
    query: search.query,
  });
}

/**
 * The cards follow the address a beat behind: the next page keeps the
 * current one on screen, dimmed, until it is in.
 */
function ProjectsPage() {
  const tProjects = useTranslations('projects');
  const format = useFormatter();
  const live = Route.useSearch();
  const search = useDeferredValue(live);
  const navigate = Route.useNavigate();
  const { data: projects } = useSuspenseQuery(projectsPage(search));
  const entry = useEntryView(
    undefined,
    'projects',
    live,
    validateProjectsSearch,
  );
  if (entry) {
    return <Navigate to="/projects" search={entry} replace />;
  }

  // Deleting the last card on the last page leaves that page empty, and an
  // empty page renders "no projects" with no way back. Send the browser to
  // the last page that exists, as the releases list does. Only once the
  // cards on screen are the address's: an older page's answer must not
  // overrule a page the reader has since asked for.
  const page = search.page ?? 1;
  if (search === live && page > 1 && page > projects.total_pages) {
    return (
      <Navigate
        to="/projects"
        search={{ ...search, page: Math.max(projects.total_pages, 1) }}
        replace
      />
    );
  }

  return (
    <ProjectsScreen
      summary={projectsSummary(projects, tProjects, format)}
      toolbar={<ProjectsToolbar search={live} />}
      views={<ProjectsViews search={live} />}
    >
      <div
        aria-busy={search !== live}
        className={cn(
          'flex flex-1 flex-col gap-4',
          search !== live && 'opacity-60',
        )}
      >
        <ProjectsGrid
          page={projects}
          currentPage={page}
          loading={search !== live}
          onPageChange={(next) =>
            navigate({ search: (previous) => ({ ...previous, page: next }) })
          }
        />
      </div>
    </ProjectsScreen>
  );
}

/** The workspace's saved views of this page. */
function ProjectsViews({ search }: { search: ProjectsSearch }) {
  const navigate = Route.useNavigate();
  return (
    <SavedViews
      page="projects"
      search={{ ...search }}
      onApply={(next) => navigate({ search: validateProjectsSearch(next) })}
    />
  );
}

const NO_FIELDS: QueryField[] = [];
const NO_FILTERS: ColumnFiltersState = [];

/** Search by name or slug, and the order the cards come in. */
function ProjectsToolbar({ search }: { search: ProjectsSearch }) {
  const t = useTranslations('projectPages');
  const navigate = Route.useNavigate();
  const sortLabels: Record<ProjectsSort, string> = {
    newest: t('projectsList.sort.newest'),
    recent: t('projectsList.sort.recent'),
    oldest: t('projectsList.sort.oldest'),
    name: t('projectsList.sort.name'),
  };

  return (
    <div className="flex flex-wrap items-center gap-2">
      <QueryBar
        className="min-w-0 max-sm:basis-full"
        fields={NO_FIELDS}
        filters={NO_FILTERS}
        search={search.query ?? ''}
        placeholder={t('projectsList.search')}
        onChange={(next) =>
          navigate({
            search: (previous) => ({
              ...previous,
              query: next.search || undefined,
              page: undefined,
            }),
          })
        }
      />
      <ScopeBar>
        <ScopeSegment
          icon={SelectorIcon}
          label={sortLabels[search.sort ?? 'newest']}
          actions={SORT_CHOICES.map((choice) => ({
            id: choice,
            label: sortLabels[choice],
            selected: choice === (search.sort ?? 'newest'),
            onSelect: () =>
              navigate({
                search: (previous) => ({
                  ...previous,
                  sort: choice === 'newest' ? undefined : choice,
                  page: undefined,
                }),
              }),
          }))}
        />
      </ScopeBar>
    </div>
  );
}

/** The same header, with a card's outline where each project goes. */
function ProjectsLoading() {
  return (
    <PageLoading>
      <ProjectsScreen
        summary={[<Skeleton key="summary" className="h-4 w-48" />]}
      >
        <div className="grid grid-cols-1 gap-3.5 lg:grid-cols-2 2xl:grid-cols-3">
          {Array.from({ length: 6 }, (_, card) => (
            // Placeholder cards have no identity beyond their position.
            // react-doctor-disable-next-line react-doctor/no-array-index-as-key
            <Skeleton key={card} className="h-36 rounded-2xl" />
          ))}
        </div>
      </ProjectsScreen>
    </PageLoading>
  );
}

function ProjectsScreen({
  summary,
  toolbar,
  views,
  children,
}: {
  summary: ReactNode[];
  toolbar?: ReactNode;
  views?: ReactNode;
  children: ReactNode;
}) {
  const t = useTranslations('projectPages');
  const canCreate = usePermissionsLater().canCreateProject;
  return (
    <div className="flex h-full flex-col">
      <title>{t('projectsList.meta.title')}</title>
      <PageHeader
        title={t('projectsList.title')}
        summary={summary}
        actions={
          <div className="flex flex-wrap items-center gap-2">
            {views}
            {canCreate ? (
              <Button
                variant="primary"
                icon={NewIcon}
                render={<Link href="/projects/new" />}
              >
                {t('projectsList.newProject')}
              </Button>
            ) : null}
          </div>
        }
      />
      <Page>
        {toolbar}
        {children}
      </Page>
    </div>
  );
}

/**
 * The header's facts: how many projects, how many need looking at, how busy
 * the last day was. The last two can only add up the page in hand, so they
 * appear only while that page is every project: beside a total for all of
 * them, a sum over twenty would read as a sum over all.
 */
function projectsSummary(
  data: ProjectsListResponse,
  tProjects: ReturnType<typeof useTranslations<'projects'>>,
  format: ReturnType<typeof useFormatter>,
): string[] {
  const count = tProjects('switcher.count', { count: data.total_count });
  if (data.total_pages > 1) return [count];

  const fatal = data.items.filter((p) => (p.stats?.fatal_issues ?? 0) > 0);
  const events = data.items.reduce(
    (sum, p) => sum + (p.stats?.events.current ?? 0),
    0,
  );

  return [
    count,
    fatal.length > 0
      ? tProjects('summary.withFatal', { count: fatal.length })
      : '',
    tProjects('summary.events24h', {
      count: events,
      display: format.number(events, 'compact'),
    }),
  ].filter(Boolean);
}
