import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { listIssues } from '@/features/issue/api/queries';
import { IssuesList } from '@/features/issue/ui/components/issues-list/issues-list';
import { getProject } from '@/features/project/api/queries';
import { translator } from '@/shared/i18n/intl';
import { loadAll } from '@/shared/lib/results';
import {
  searchOneOf,
  searchPage,
  searchString,
} from '@/shared/lib/search-params';
import { LoadFailure } from '@/shared/ui/components/load-failure';

const FILTERS = ['open', 'resolved', 'muted', 'all'] as const;

export const Route = createFileRoute('/_authenticated/projects/$id/issues/')({
  validateSearch: (search: Record<string, unknown>) => ({
    filter: searchOneOf(search.filter, FILTERS),
    page: searchPage(search.page),
    environment: searchString(search.environment),
  }),
  loaderDeps: ({ search }) => ({
    filter: search.filter ?? 'open',
    page: search.page ?? 1,
    environment: search.environment,
  }),
  loader: ({ params, deps }) => {
    const projectId = Number.parseInt(params.id, 10);
    return loadAll([
      getProject(projectId),
      listIssues(projectId, {
        filter: deps.filter,
        page: deps.page,
        per_page: 20,
        sort: 'last_seen',
        order: 'desc',
        environment: deps.environment,
      }),
    ]);
  },
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
  const { id } = Route.useParams();
  const { filter, page, environment } = Route.useSearch({
    select: (search) => ({
      filter: search.filter ?? 'open',
      page: search.page ?? 1,
      environment: search.environment,
    }),
  });
  const loaded = Route.useLoaderData();
  const projectId = Number.parseInt(id, 10);

  if (!loaded.success) {
    return <LoadFailure error={loaded.error} title={t('issues.loadFailed')} />;
  }

  const [project, issuesResponse] = loaded.data;

  return (
    <div className="flex flex-col h-full">
      <div className="shrink-0 w-full px-4 md:px-8 py-4 md:py-6 border-b">
        <h1 className="text-lg font-semibold">{t('issues.title')}</h1>
        <p className="text-sm text-muted-foreground mt-0.5">
          {t('issues.subtitle', { project: project.name })}
        </p>
      </div>

      <div className="flex-1 overflow-hidden w-full px-4 md:px-8 py-4 md:py-6">
        <IssuesList
          projectId={projectId}
          initialIssues={issuesResponse}
          currentFilter={filter}
          currentPage={page}
          environment={environment}
        />
      </div>
    </div>
  );
}
