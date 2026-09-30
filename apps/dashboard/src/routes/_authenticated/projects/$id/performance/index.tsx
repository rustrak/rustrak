import { createFileRoute } from '@tanstack/react-router';
import { Zap } from 'lucide-react';
import { useTranslations } from 'use-intl';
import { getProject } from '@/features/project/api/queries';
import { getTransactionStats } from '@/features/transaction/api/queries';
import { TransactionStatsTable } from '@/features/transaction/ui/components/transaction-stats-table';
import { translator } from '@/shared/i18n/intl';
import { searchPage, searchString } from '@/shared/lib/search-params';
import { LoadFailure } from '@/shared/ui/components/load-failure';

export const Route = createFileRoute(
  '/_authenticated/projects/$id/performance/',
)({
  validateSearch: (search: Record<string, unknown>) => ({
    page: searchPage(search.page),
    environment: searchString(search.environment),
  }),
  loaderDeps: ({ search }) => ({
    page: search.page ?? 1,
    environment: search.environment,
  }),
  loader: async ({ params, deps }) => {
    const projectId = Number.parseInt(params.id, 10);
    const project = await getProject(projectId);

    if (!project.success) return { project, stats: null };

    // Nothing is swallowed: a fetch/auth failure renders an outage surface
    // rather than the "no transactions yet" onboarding state.
    const stats = await getTransactionStats(projectId, {
      page: deps.page,
      per_page: 20,
      environment: deps.environment,
    });

    return { project, stats };
  },
  head: ({ loaderData }) => {
    const t = translator('projectPages');

    if (!loaderData?.project.success) {
      return { meta: [{ title: t('projectNotFound') }] };
    }

    const project = loaderData.project.data;
    return {
      meta: [
        { title: t('performance.meta.title', { project: project.name }) },
        {
          name: 'description',
          content: t('performance.meta.description', { project: project.name }),
        },
      ],
    };
  },
  component: PerformancePage,
});

function PerformancePage() {
  const t = useTranslations('projectPages');
  const { id } = Route.useParams();
  const { currentPage, environment } = Route.useSearch({
    select: (search) => ({
      currentPage: search.page ?? 1,
      environment: search.environment,
    }),
  });
  const { project: projectResult, stats: statsResult } = Route.useLoaderData();
  const projectId = Number.parseInt(id, 10);

  if (!projectResult.success) {
    return (
      <LoadFailure error={projectResult.error} title={t('loadProjectFailed')} />
    );
  }

  const project = projectResult.data;

  // `null` only where the project read already failed, and that branch
  // returned above — so this one is a real failure of its own request.
  if (statsResult === null) return null;

  if (!statsResult.success) {
    return (
      <LoadFailure
        error={statsResult.error}
        title={t('performance.loadFailed')}
        notFoundOnMissing={false}
      />
    );
  }

  const stats = statsResult.data;

  return (
    <div className="flex flex-col h-full">
      <div className="shrink-0 w-full px-4 md:px-8 py-4 md:py-6 border-b">
        <h1 className="text-lg font-semibold">{t('performance.title')}</h1>
        <p className="text-sm text-muted-foreground mt-0.5">
          {t('performance.subtitle', { project: project.name })}
        </p>
      </div>

      <div className="flex-1 overflow-hidden w-full px-4 md:px-8 py-4 md:py-6">
        {stats.total_count === 0 && !environment ? (
          <div className="flex flex-col items-center justify-center min-h-full text-center">
            <Zap className="size-12 text-muted-foreground/30 mb-4" />
            <h2 className="text-lg font-semibold mb-1">
              {t('performance.emptyTitle')}
            </h2>
            <p className="text-sm text-muted-foreground max-w-md">
              {t.rich('performance.emptyDescription', {
                code: (chunks) => <code>{chunks}</code>,
              })}
            </p>
          </div>
        ) : stats.total_count === 0 ? (
          <div className="flex flex-col items-center justify-center min-h-full text-center">
            <Zap className="size-12 text-muted-foreground/30 mb-4" />
            <h2 className="text-lg font-semibold mb-1">
              {t('performance.noEnvironmentData')}
            </h2>
          </div>
        ) : (
          <TransactionStatsTable
            projectId={projectId}
            stats={stats.items}
            currentPage={currentPage}
            totalPages={stats.total_pages}
            totalCount={stats.total_count}
            perPage={stats.per_page}
          />
        )}
      </div>
    </div>
  );
}
