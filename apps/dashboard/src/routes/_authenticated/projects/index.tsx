import type { ProjectsListResponse } from '@rustrak/client';
import { Button, cn, NewIcon, Page, PageHeader, Skeleton } from '@rustrak/ui';
import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, Navigate } from '@tanstack/react-router';
import { type ReactNode, useDeferredValue } from 'react';
import { useFormatter, useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { ProjectsGrid } from '@/features/project/ui/components/projects-grid';
import { searchPage } from '@/shared/lib/search-params';
import { Link } from '@/shared/ui/components/link';
import { PageLoading } from '@/shared/ui/components/page-loading';

/**
 * Window for the per-row stats.
 *
 * Fixed rather than user-selectable: this page answers "which project should
 * I look at", and `/projects/[id]` already owns choosing a time range.
 */
const STATS_PERIOD = '24h';

export const Route = createFileRoute('/_authenticated/projects/')({
  validateSearch: (search: Record<string, unknown>) => ({
    page: searchPage(search.page),
  }),
  loaderDeps: ({ search }) => ({ page: search.page ?? 1 }),
  // One request, stats included: the server aggregates the whole page in two
  // queries. Asking per row would be 20 round trips for a table that renders
  // above the fold.
  loader: ({ context: { queryClient }, deps }) => {
    void queryClient.query(projectsPage(deps.page)).catch(noop);
  },
  pendingComponent: ProjectsLoading,
  component: ProjectsPage,
});

function projectsPage(page: number) {
  return projectQueries.list({
    page,
    per_page: 20,
    stats_period: STATS_PERIOD,
  });
}

/**
 * The cards follow the address a beat behind: the next page keeps the
 * current one on screen, dimmed, until it is in.
 */
function ProjectsPage() {
  const tProjects = useTranslations('projects');
  const format = useFormatter();
  const live = Route.useSearch({ select: (search) => search.page ?? 1 });
  const page = useDeferredValue(live);
  const { data: projects } = useSuspenseQuery(projectsPage(page));

  // Deleting the last card on the last page leaves that page empty, and an
  // empty page renders "no projects" with no way back. Send the browser to
  // the last page that exists, as the releases list does. Only once the
  // cards on screen are the address's: an older page's answer must not
  // overrule a page the reader has since asked for.
  if (page === live && page > 1 && page > projects.total_pages) {
    return (
      <Navigate
        to="/projects"
        search={{ page: Math.max(projects.total_pages, 1) }}
        replace
      />
    );
  }

  return (
    <ProjectsScreen summary={projectsSummary(projects, tProjects, format)}>
      <div
        aria-busy={page !== live}
        className={cn(
          'flex flex-1 flex-col gap-4',
          page !== live && 'opacity-60',
        )}
      >
        <ProjectsGrid
          page={projects}
          currentPage={page}
          loading={page !== live}
        />
      </div>
    </ProjectsScreen>
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
  children,
}: {
  summary: ReactNode[];
  children: ReactNode;
}) {
  const t = useTranslations('projectPages');
  return (
    <div className="flex h-full flex-col">
      <title>{t('projectsList.meta.title')}</title>
      <PageHeader
        title={t('projectsList.title')}
        summary={summary}
        actions={
          <Button
            variant="primary"
            icon={NewIcon}
            render={<Link href="/projects/new" />}
          >
            {t('projectsList.newProject')}
          </Button>
        }
      />
      <Page>{children}</Page>
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
