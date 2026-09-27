import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { ProjectsList } from '@/features/project/ui/components/projects-list/projects-list';
import { translator } from '@/shared/i18n/intl';
import { searchPage } from '@/shared/lib/search-params';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import { ProjectsHeader } from './-components/projects-header';

/**
 * Window for the per-row stats.
 *
 * Fixed rather than user-selectable: this page answers "which project should
 * I look at", and `/projects/[id]` already owns choosing a time range.
 */
const STATS_PERIOD = '24h';

function pageQuery(page: number) {
  return projectQueries.list({
    page,
    per_page: 20,
    stats_period: STATS_PERIOD,
  });
}

export const Route = createFileRoute('/_authenticated/projects/')({
  head: () => {
    const t = translator('projectPages');
    return {
      meta: [
        { title: t('projectsList.meta.title') },
        {
          name: 'description',
          content: t('projectsList.meta.description'),
        },
      ],
    };
  },
  validateSearch: (search: Record<string, unknown>): { page?: number } => ({
    page: searchPage(search.page),
  }),
  loaderDeps: ({ search }) => ({ page: search.page ?? 1 }),
  // One request, stats included: the server aggregates the whole page in two
  // queries. Asking per row would be 20 round trips for a table that renders
  // above the fold.
  loader: ({ deps, context: { queryClient } }) =>
    queryClient.ensureQueryData(pageQuery(deps.page)),
  component: ProjectsPage,
});

function ProjectsPage() {
  const t = useTranslations('projectPages');
  const page = Route.useSearch({ select: (search) => search.page ?? 1 });
  const { data: projectsResponse } = useSuspenseQuery(pageQuery(page));

  if (!projectsResponse.success) {
    return (
      <LoadFailure
        error={projectsResponse.error}
        title={t('projectsList.loadFailed')}
        notFoundOnMissing={false}
      />
    );
  }

  return (
    <div className="flex flex-col h-[calc(100vh-64px)]">
      {/* Header section - fixed */}
      <div className="shrink-0 w-full px-4 md:px-8 py-4 md:py-6 border-b">
        <ProjectsHeader />
      </div>

      {/* Content section - grows and handles overflow */}
      <div className="flex-1 overflow-hidden w-full px-4 md:px-8 py-4 md:py-6">
        <ProjectsList
          initialProjects={projectsResponse.data}
          currentPage={page}
        />
      </div>
    </div>
  );
}
