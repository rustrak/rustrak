import type { IssueFilter } from '@rustrak/client';
import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { issueQueries } from '@/features/issue/api/queries';
import { IssuesList } from '@/features/issue/ui/components/issues-list/issues-list';
import { projectQueries } from '@/features/project/api/queries';
import { translator } from '@/shared/i18n/intl';
import { combine, loadAll } from '@/shared/lib/results';
import { searchOneOf, searchPage } from '@/shared/lib/search-params';
import { LoadFailure } from '@/shared/ui/components/load-failure';

const FILTERS = ['open', 'resolved', 'muted', 'all'] as const;

function issuesQuery(projectId: number, filter: IssueFilter, page: number) {
  return issueQueries.list(projectId, {
    filter,
    page,
    per_page: 20,
    sort: 'last_seen',
    order: 'desc',
  });
}

export const Route = createFileRoute('/_authenticated/projects/$id/issues/')({
  validateSearch: (
    search: Record<string, unknown>,
  ): {
    filter?: IssueFilter;
    page?: number;
  } => ({
    filter: searchOneOf(search.filter, FILTERS),
    page: searchPage(search.page),
  }),
  loaderDeps: ({ search }) => ({
    filter: search.filter ?? 'open',
    page: search.page ?? 1,
  }),
  loader: ({ params: { id }, deps, context: { queryClient } }) =>
    loadAll([
      queryClient.ensureQueryData(projectQueries.detail(id)),
      queryClient.ensureQueryData(issuesQuery(id, deps.filter, deps.page)),
    ]),
  head: ({ loaderData }) => {
    const t = translator('projectPages');

    if (!loaderData?.success) {
      return { meta: [{ title: t('projectNotFound') }] };
    }

    const [project] = loaderData.data;
    return {
      meta: [
        { title: t('projectTitle', { project: project.name }) },
        {
          name: 'description',
          content: t('issues.meta.description', { project: project.name }),
        },
      ],
    };
  },
  component: IssuesPage,
});

function IssuesPage() {
  const t = useTranslations('projectPages');
  const { id: projectId } = Route.useParams();
  const { filter, page } = Route.useSearch({
    select: (search) => ({
      filter: search.filter ?? 'open',
      page: search.page ?? 1,
    }),
  });
  const loaded = combine([
    useSuspenseQuery(projectQueries.detail(projectId)).data,
    useSuspenseQuery(issuesQuery(projectId, filter, page)).data,
  ]);

  if (!loaded.success) {
    return <LoadFailure error={loaded.error} title={t('issues.loadFailed')} />;
  }

  const [project, issuesResponse] = loaded.data;

  return (
    <div className="flex flex-col h-[calc(100vh-64px)]">
      <div className="shrink-0 w-full px-4 md:px-8 py-4 md:py-6 border-b">
        <h1 className="text-lg font-semibold">{t('issues.title')}</h1>
        <p className="text-sm text-muted-foreground mt-0.5">
          {t('issues.subtitle', { project: project.name })}
        </p>
      </div>

      <div className="flex-1 overflow-hidden w-full px-4 md:px-8 py-4 md:py-6">
        <IssuesList
          projectId={projectId}
          issues={issuesResponse}
          currentFilter={filter}
          currentPage={page}
        />
      </div>
    </div>
  );
}
