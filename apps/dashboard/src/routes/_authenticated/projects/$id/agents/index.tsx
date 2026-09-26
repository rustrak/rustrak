import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { Bot } from 'lucide-react';
import { useTranslations } from 'use-intl';
import { agentQueries } from '@/features/agent-trace/api/queries';
import { resolveAgentFilters } from '@/features/agent-trace/model/filters';
import { AgentBreakdownChart } from '@/features/agent-trace/ui/components/agent-breakdown-chart';
import { AgentDashboardFilters } from '@/features/agent-trace/ui/components/agent-dashboard-filters';
import { AgentDurationChart } from '@/features/agent-trace/ui/components/agent-duration-chart';
import { AgentModelsTable } from '@/features/agent-trace/ui/components/agent-models-table';
import { AgentSummaryTiles } from '@/features/agent-trace/ui/components/agent-summary-tiles';
import { AgentTimeseriesChart } from '@/features/agent-trace/ui/components/agent-timeseries-chart';
import { AgentToolsTable } from '@/features/agent-trace/ui/components/agent-tools-table';
import { AgentTracesTable } from '@/features/agent-trace/ui/components/agent-traces-table';
import { projectQueries } from '@/features/project/api/queries';
import { translator } from '@/shared/i18n/intl';
import { searchPage, searchString } from '@/shared/lib/search-params';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/shared/ui/components/shadcn/card';

// One window for every widget on the page: a chart on 24h beside a table on
// all-time is a reader's trap, not a feature.
function dashboardQuery(
  projectId: number,
  search: { page: number; period?: string; environment?: string },
) {
  return agentQueries.dashboard(
    projectId,
    resolveAgentFilters({
      period: search.period,
      environment: search.environment,
    }),
    search.page,
  );
}

export const Route = createFileRoute('/_authenticated/projects/$id/agents/')({
  validateSearch: (
    search: Record<string, unknown>,
  ): {
    page?: number;
    period?: string;
    environment?: string;
  } => ({
    page: searchPage(search.page),
    period: searchString(search.period),
    environment: searchString(search.environment),
  }),
  loaderDeps: ({ search }) => ({
    page: search.page ?? 1,
    period: search.period,
    environment: search.environment,
  }),
  loader: async ({ params: { id }, deps, context: { queryClient } }) => {
    // Nothing is swallowed: a fetch/auth failure renders an outage surface
    // rather than the "no agent activity yet" onboarding state, which would
    // tell a team whose agents are running that they never instrumented
    // anything.
    const [project] = await Promise.all([
      queryClient.ensureQueryData(projectQueries.detail(id)),
      queryClient.ensureQueryData(dashboardQuery(id, deps)),
    ]);
    return { project };
  },
  head: ({ loaderData }) => {
    const t = translator('projectPages');

    if (!loaderData?.project.success) {
      return { meta: [{ title: t('projectNotFound') }] };
    }

    const project = loaderData.project.data;
    return {
      meta: [
        { title: t('agents.meta.title', { project: project.name }) },
        {
          name: 'description',
          content: t('agents.meta.description', { project: project.name }),
        },
      ],
    };
  },
  component: AgentsPage,
});

function AgentsPage() {
  const t = useTranslations('projectPages');
  const { id: projectId } = Route.useParams();
  const { currentPage, period, environment } = Route.useSearch({
    select: (search) => ({
      currentPage: search.page ?? 1,
      period: search.period,
      environment: search.environment,
    }),
  });
  const projectResult = useSuspenseQuery(projectQueries.detail(projectId)).data;
  const loaded = useSuspenseQuery(
    dashboardQuery(projectId, { page: currentPage, period, environment }),
  ).data;

  if (!projectResult.success) {
    return (
      <LoadFailure error={projectResult.error} title={t('loadProjectFailed')} />
    );
  }

  const project = projectResult.data;

  if (!loaded.success) {
    return (
      <LoadFailure
        error={loaded.error}
        title={t('agents.loadFailed')}
        notFoundOnMissing={false}
      />
    );
  }

  const [
    runs,
    duration,
    modelsByCalls,
    modelsByTokens,
    tools,
    traces,
    summary,
    modelRows,
    toolRows,
    environments,
  ] = loaded.data;

  return (
    <div className="flex flex-col h-[calc(100vh-64px)]">
      <div className="shrink-0 w-full px-4 md:px-8 py-4 md:py-6 border-b">
        <h1 className="text-lg font-semibold">{t('agents.title')}</h1>
        <p className="text-sm text-muted-foreground mt-0.5">
          {t('agents.subtitle', { project: project.name })}
        </p>
        <div className="mt-3">
          <AgentDashboardFilters
            projectId={projectId}
            current={{ period, environment }}
            environments={environments}
          />
        </div>
      </div>

      <div className="flex-1 overflow-auto w-full px-4 md:px-8 py-4 md:py-6">
        {traces.total_count === 0 ? (
          <div className="flex flex-col items-center justify-center min-h-full text-center">
            <Bot className="size-12 text-muted-foreground/30 mb-4" />
            <h2 className="text-lg font-semibold mb-1">
              {t('agents.emptyTitle')}
            </h2>
            <p className="text-sm text-muted-foreground max-w-md">
              {t('agents.emptyDescription')}
            </p>
          </div>
        ) : (
          <div className="flex flex-col gap-4">
            <AgentSummaryTiles summary={summary} />

            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <Card size="sm">
                <CardHeader>
                  <CardTitle>{t('agents.cardRuns')}</CardTitle>
                  <CardDescription>
                    {t('agents.cardRunsDescription')}
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  <AgentTimeseriesChart points={runs} />
                </CardContent>
              </Card>

              <Card size="sm">
                <CardHeader>
                  <CardTitle>{t('agents.cardDuration')}</CardTitle>
                  <CardDescription>
                    {t('agents.cardDurationDescription')}
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  <AgentDurationChart points={duration} />
                </CardContent>
              </Card>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <Card size="sm">
                <CardHeader>
                  <CardTitle>{t('agents.cardModelsByCalls')}</CardTitle>
                  <CardDescription>
                    {t('agents.cardModelsByCallsDescription')}
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  <AgentBreakdownChart rows={modelsByCalls} />
                </CardContent>
              </Card>

              <Card size="sm">
                <CardHeader>
                  <CardTitle>{t('agents.cardModelsByTokens')}</CardTitle>
                  <CardDescription>
                    {t('agents.cardModelsByTokensDescription')}
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  <AgentBreakdownChart rows={modelsByTokens} />
                </CardContent>
              </Card>

              <Card size="sm">
                <CardHeader>
                  <CardTitle>{t('agents.cardTools')}</CardTitle>
                  <CardDescription>
                    {t('agents.cardToolsDescription')}
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  <AgentBreakdownChart rows={tools} />
                </CardContent>
              </Card>
            </div>

            <div className="grid grid-cols-1 xl:grid-cols-2 gap-4">
              <Card size="sm">
                <CardHeader>
                  <CardTitle>{t('agents.cardModelsTable')}</CardTitle>
                  <CardDescription>
                    {t('agents.cardModelsTableDescription')}
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  <AgentModelsTable rows={modelRows} />
                </CardContent>
              </Card>

              <Card size="sm">
                <CardHeader>
                  <CardTitle>{t('agents.cardToolsTable')}</CardTitle>
                  <CardDescription>
                    {t('agents.cardToolsTableDescription')}
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  <AgentToolsTable rows={toolRows} />
                </CardContent>
              </Card>
            </div>

            <Card size="sm">
              <CardHeader>
                <CardTitle>{t('agents.cardTraces')}</CardTitle>
                <CardDescription>
                  {t('agents.cardTracesDescription')}
                </CardDescription>
              </CardHeader>
              <CardContent>
                <AgentTracesTable
                  projectId={projectId}
                  traces={traces.items}
                  currentPage={Math.min(
                    currentPage,
                    Math.max(1, traces.total_pages),
                  )}
                  totalPages={traces.total_pages}
                  totalCount={traces.total_count}
                  perPage={traces.per_page}
                />
              </CardContent>
            </Card>
          </div>
        )}
      </div>
    </div>
  );
}
