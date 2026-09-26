import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { ScrollText } from 'lucide-react';
import { useTranslations } from 'use-intl';
import { logQueries } from '@/features/log/api/queries';
import { LogsList } from '@/features/log/ui/components/logs-list';
import { projectQueries } from '@/features/project/api/queries';
import { translator } from '@/shared/i18n/intl';
import { searchPage, searchString } from '@/shared/lib/search-params';
import { LoadFailure } from '@/shared/ui/components/load-failure';

function logsQuery(projectId: number, page: number, level?: string) {
  return logQueries.list(projectId, { page, per_page: 50, level });
}

export const Route = createFileRoute('/_authenticated/projects/$id/logs')({
  validateSearch: (
    search: Record<string, unknown>,
  ): { page?: number; level?: string } => ({
    page: searchPage(search.page),
    level: searchString(search.level),
  }),
  loaderDeps: ({ search }) => ({ page: search.page ?? 1, level: search.level }),
  loader: async ({ params: { id }, deps, context: { queryClient } }) => {
    // Nothing is swallowed: a fetch/auth failure renders an outage surface
    // rather than the "no logs yet" onboarding state.
    const [project, logs] = await Promise.all([
      queryClient.ensureQueryData(projectQueries.detail(id)),
      queryClient.ensureQueryData(logsQuery(id, deps.page, deps.level)),
    ]);
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
  const { id: projectId } = Route.useParams();
  const { page, level } = Route.useSearch({
    select: (search) => ({ page: search.page ?? 1, level: search.level }),
  });
  const projectResult = useSuspenseQuery(projectQueries.detail(projectId)).data;
  const logsResult = useSuspenseQuery(logsQuery(projectId, page, level)).data;

  if (!projectResult.success) {
    return (
      <LoadFailure error={projectResult.error} title={t('loadProjectFailed')} />
    );
  }

  const project = projectResult.data;

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
    <div className="flex flex-col h-[calc(100vh-64px)]">
      <div className="shrink-0 w-full px-4 md:px-8 py-4 md:py-6 border-b">
        <h1 className="text-lg font-semibold">{t('logs.title')}</h1>
        <p className="text-sm text-muted-foreground mt-0.5">
          {t('logs.subtitle', { project: project.name })}
        </p>
      </div>

      <div className="flex-1 overflow-hidden w-full px-4 md:px-8 py-4 md:py-6">
        {logs.total_count === 0 && !level ? (
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
          <LogsList initialLogs={logs} currentPage={page} activeLevel={level} />
        )}
      </div>
    </div>
  );
}
