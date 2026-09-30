import { createFileRoute } from '@tanstack/react-router';
import { ScrollText } from 'lucide-react';
import { useTranslations } from 'use-intl';
import { listLogs } from '@/features/log/api/queries';
import { LogsList } from '@/features/log/ui/components/logs-list';
import { getProject } from '@/features/project/api/queries';
import { translator } from '@/shared/i18n/intl';
import { searchPage, searchString } from '@/shared/lib/search-params';
import { LoadFailure } from '@/shared/ui/components/load-failure';

export const Route = createFileRoute('/_authenticated/projects/$id/logs')({
  validateSearch: (search: Record<string, unknown>) => ({
    page: searchPage(search.page),
    level: searchString(search.level),
    environment: searchString(search.environment),
  }),
  loaderDeps: ({ search }) => ({
    page: search.page ?? 1,
    level: search.level,
    environment: search.environment,
  }),
  loader: async ({ params, deps }) => {
    const projectId = Number.parseInt(params.id, 10);
    const project = await getProject(projectId);

    if (!project.success) return { project, logs: null };

    // Nothing is swallowed: a fetch/auth failure renders an outage surface
    // rather than the "no logs yet" onboarding state.
    const logs = await listLogs(projectId, {
      page: deps.page,
      per_page: 50,
      level: deps.level,
      environment: deps.environment,
    });

    return { project, logs };
  },
  head: ({ loaderData }) => {
    const t = translator('projectPages');

    if (!loaderData?.project.success) {
      return { meta: [{ title: t('projectNotFound') }] };
    }

    const project = loaderData.project.data;
    return {
      meta: [
        { title: t('logs.meta.title', { project: project.name }) },
        {
          name: 'description',
          content: t('logs.meta.description', { project: project.name }),
        },
      ],
    };
  },
  component: LogsPage,
});

function LogsPage() {
  const t = useTranslations('projectPages');
  const { id } = Route.useParams();
  const { page, level, environment } = Route.useSearch({
    select: (search) => ({
      page: search.page ?? 1,
      level: search.level,
      environment: search.environment,
    }),
  });
  const { project: projectResult, logs: logsResult } = Route.useLoaderData();
  const projectId = Number.parseInt(id, 10);

  if (!projectResult.success) {
    return (
      <LoadFailure error={projectResult.error} title={t('loadProjectFailed')} />
    );
  }

  const project = projectResult.data;

  // `null` only where the project read already failed, and that branch
  // returned above — so this one is a real failure of its own request.
  if (logsResult === null) return null;

  if (!logsResult.success) {
    return (
      <LoadFailure
        error={logsResult.error}
        title={t('logs.loadFailed')}
        notFoundOnMissing={false}
      />
    );
  }

  const logs = logsResult.data;

  return (
    <div className="flex flex-col h-full">
      <div className="shrink-0 w-full px-4 md:px-8 py-4 md:py-6 border-b">
        <h1 className="text-lg font-semibold">{t('logs.title')}</h1>
        <p className="text-sm text-muted-foreground mt-0.5">
          {t('logs.subtitle', { project: project.name })}
        </p>
      </div>

      <div className="flex-1 overflow-hidden w-full px-4 md:px-8 py-4 md:py-6">
        {logs.total_count === 0 && !level && !environment ? (
          <div className="flex flex-col items-center justify-center min-h-full text-center">
            <ScrollText className="size-12 text-muted-foreground/30 mb-4" />
            <h2 className="text-lg font-semibold mb-1">
              {t('logs.emptyTitle')}
            </h2>
            <p className="text-sm text-muted-foreground max-w-md">
              {t.rich('logs.emptyDescription', {
                code: (chunks) => <code>{chunks}</code>,
              })}
            </p>
          </div>
        ) : (
          <LogsList
            projectId={projectId}
            initialLogs={logs}
            currentPage={page}
            activeLevel={level}
          />
        )}
      </div>
    </div>
  );
}
