import type {
  AgentSummary,
  AgentsConversationsResponse,
  AgentsTracesData,
  AgentsTracesResponse,
  ProjectResponse,
} from '@rustrak/client';
import {
  type FilterVariants,
  Page,
  PageHeader,
  Skeleton,
  Tab,
  TabList,
  Tabs,
} from '@rustrak/ui';
import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, Navigate } from '@tanstack/react-router';
import { type ReactNode, useDeferredValue } from 'react';
import { useTranslations } from 'use-intl';
import { agentQueries } from '@/features/agent-trace/api/queries';
import {
  AGENT_PERIOD_PARAMS,
  agentPeriodParam,
  fillBuckets,
  isNothingYet,
  resolveAgentFilters,
} from '@/features/agent-trace/model/filters';
import { ConversationsList } from '@/features/agent-trace/ui/components/agent-conversations';
import {
  AgentFigures,
  DurationChart,
  ModelsTable,
  RunsChart,
  ToolsTable,
} from '@/features/agent-trace/ui/components/agent-overview';
import { AgentScope } from '@/features/agent-trace/ui/components/agent-scope';
import { AgentTracesTable } from '@/features/agent-trace/ui/components/agent-traces-table';
import { projectQueries } from '@/features/project/api/queries';
import { savedViewQueries } from '@/features/saved-view/api/queries';
import { SavedViews } from '@/features/saved-view/ui/components/saved-views';
import { useEntryView } from '@/features/saved-view/ui/hooks/use-entry-view';
import {
  searchOneOf,
  searchPage,
  searchString,
} from '@/shared/lib/search-params';
import {
  filterValue,
  type TableSearch,
  tableSearch,
} from '@/shared/lib/table-search';
import { EmptyState } from '@/shared/ui/components/empty-state';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { Panel, PanelPlaceholder } from '@/shared/ui/components/panel';
import { QueryState } from '@/shared/ui/components/query-state';
import { useRouter } from '@/shared/ui/hooks/use-router';

const PAGE_SIZE = 25;
const VIEWS = ['overview', 'traces', 'conversations'] as const;

/** The traces table's keys; free text has no column to search. */
const VARIANTS: FilterVariants = {
  agent: 'text',
  model: 'options',
  tool: 'options',
  status: 'options',
};

type TraceSort = NonNullable<NonNullable<AgentsTracesData['query']>['sort']>;
const SORTS: TraceSort[] = [
  'newest',
  'duration',
  'tokens',
  'llm_calls',
  'tool_calls',
  'errors',
];

interface AgentsSearch extends Omit<TableSearch, 'per'> {
  view?: (typeof VIEWS)[number];
  /** Absent is the last 24 hours; `all` is all time. */
  period?: (typeof AGENT_PERIOD_PARAMS)[number];
  environment?: string;
}

const table = tableSearch({
  variants: VARIANTS,
  pageSize: PAGE_SIZE,
  // One known ranking, always from the most down, at a fixed page size.
  normalise: (query) => ({
    ...query,
    sorting: query.sorting
      .filter((s) => SORTS.includes(s.id as TraceSort))
      .slice(0, 1)
      .map((s) => ({ ...s, desc: true })),
    pagination: { ...query.pagination, pageSize: PAGE_SIZE },
  }),
  defaultSort: { id: 'newest', desc: true },
});

/** The traces table takes no `?per=`. */
const withoutPer = ({ per: _, ...rest }: TableSearch) => rest;

/**
 * The page's reads for a URL, built once for the loader and the components so
 * both name the same cache entries. One window for every widget: a chart on
 * 24h beside a table on all time is a reader's trap, not a feature.
 */
function agentReads(projectId: number, search: AgentsSearch) {
  const filters = resolveAgentFilters(search);
  const window = {
    period_hours: filters.periodHours,
    environment: filters.environment,
  };
  const series = { ...window, interval_hours: filters.intervalHours };
  const query = table.read(search);
  const sort = query.sorting[0]?.id as TraceSort;
  return {
    filters,
    query,
    summary: agentQueries.summary(projectId, window),
    // Not filtered by the current environment: the picker has to keep
    // offering the option you would switch back to.
    environments: agentQueries.environments(projectId),
    runs: agentQueries.runs(projectId, series),
    duration: agentQueries.duration(projectId, series),
    models: agentQueries.models(projectId, window),
    tools: agentQueries.tools(projectId, window),
    traces: agentQueries.traces(projectId, {
      ...window,
      agent: filterValue(query, 'agent'),
      model: filterValue(query, 'model'),
      tool: filterValue(query, 'tool'),
      errors: filterValue(query, 'status') === 'failing' || undefined,
      sort: sort === 'newest' ? undefined : sort,
      page: query.pagination.pageIndex + 1,
      per_page: PAGE_SIZE,
    }),
    conversations: agentQueries.conversations(projectId, {
      ...window,
      page: searchPage(search.page) ?? 1,
      per_page: PAGE_SIZE,
    }),
  };
}

/** The address as typed search; a saved view's text goes through it too. */
function validateAgentsSearch(search: Record<string, unknown>): AgentsSearch {
  return {
    view: searchOneOf(search.view, VIEWS),
    period: searchOneOf(search.period, AGENT_PERIOD_PARAMS),
    environment: searchString(search.environment),
    ...withoutPer(table.validate(search)),
  };
}

/**
 * Agents: Sentry's AI agents insights. The overview answers how the agents
 * are doing -- runs, calls, tokens, latency, failures, by model and by tool
 * -- and the traces tab lists every run, a row a click on a model or tool
 * narrows to the traces that used it.
 */
export const Route = createFileRoute('/_authenticated/projects/$id/agents/')({
  validateSearch: validateAgentsSearch,
  loaderDeps: ({ search }) => search,
  // Every read of the open tab is started here and none is waited on. The
  // header and the open tab suspend; the overview's panels land on their
  // own, so the slowest of them, or one that times out, holds up nothing.
  loader: ({ context: { queryClient }, params, deps }) => {
    const projectId = params.id;
    const reads = agentReads(projectId, deps);
    const view = deps.view ?? 'overview';
    void queryClient.query(reads.summary).catch(noop);
    void queryClient.query(reads.environments).catch(noop);
    if (view === 'overview') {
      void queryClient.query(reads.runs).catch(noop);
      void queryClient.query(reads.duration).catch(noop);
    }
    // The overview's tables, and the traces table's filter options.
    if (view !== 'conversations') {
      void queryClient.query(reads.models).catch(noop);
      void queryClient.query(reads.tools).catch(noop);
    }
    if (view === 'traces') {
      void queryClient.query(reads.traces).catch(noop);
    }
    if (view === 'conversations') {
      void queryClient.query(reads.conversations).catch(noop);
    }
    void queryClient
      .query(savedViewQueries.list(projectId, 'agents'))
      .catch(noop);
  },
  pendingComponent: AgentsLoading,
  component: AgentsPage,
});

/**
 * The header and the open tab are what the page waits for. The page follows
 * the address a beat behind: a new window, filter or page keeps what is on
 * screen, dimmed, until the next is in.
 */
function AgentsPage() {
  const tPages = useTranslations('projectPages');
  const { id: projectId } = Route.useParams();
  const live = Route.useSearch();
  const search = useDeferredValue(live);
  const navigate = Route.useNavigate();
  const loading = search !== live;
  const view = search.view ?? 'overview';
  const reads = agentReads(projectId, search);
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  const { data: summary } = useSuspenseQuery(reads.summary);
  const { data: environments } = useSuspenseQuery(reads.environments);
  const entry = useEntryView(projectId, 'agents', live, validateAgentsSearch);
  if (entry) {
    return (
      <Navigate
        to="/projects/$id/agents"
        params={{ id: projectId }}
        search={entry}
        replace
      />
    );
  }

  const scope = (
    <AgentScope
      environments={environments}
      environment={live.environment}
      period={resolveAgentFilters(live).period}
      onChange={(next) =>
        navigate({
          search: (prev) => ({
            ...prev,
            environment: next.environment,
            period: agentPeriodParam(next.period),
            page: undefined,
          }),
        })
      }
    />
  );

  const nothingYet = isNothingYet(summary, reads.filters, environments);

  return (
    <AgentsScreen project={project} summary={summary} tabs={!nothingYet}>
      {nothingYet ? (
        <EmptyState
          title={tPages('agents.emptyTitle')}
          description={tPages('agents.emptyDescription')}
        />
      ) : view === 'conversations' ? (
        <AgentsConversations
          reads={reads}
          loading={loading}
          scope={scope}
          projectId={projectId}
        />
      ) : view === 'traces' ? (
        <AgentsTraces
          reads={reads}
          loading={loading}
          scope={scope}
          projectId={projectId}
        />
      ) : (
        <AgentsOverview
          reads={reads}
          summary={summary}
          scope={scope}
          loading={loading}
        />
      )}
    </AgentsScreen>
  );
}

/**
 * The same header and tab, with the figures and rows still to come.
 */
function AgentsLoading() {
  const { id: projectId } = Route.useParams();
  const search = Route.useSearch();
  const view = search.view ?? 'overview';
  const scope = <Skeleton className="h-8 w-full sm:w-72" />;
  return (
    <PageLoading>
      <AgentsScreen tabs>
        {view === 'conversations' ? (
          <ConversationsPanel
            loading={false}
            scope={scope}
            projectId={projectId}
          />
        ) : view === 'traces' ? (
          <TracesPanel
            models={NO_NAMES}
            tools={NO_NAMES}
            loading={false}
            scope={scope}
            projectId={projectId}
          />
        ) : (
          <AgentsOverview reads={agentReads(projectId, search)} scope={scope} />
        )}
      </AgentsScreen>
    </PageLoading>
  );
}

const NO_NAMES: string[] = [];

/** The header and the tabs, the figures in it once the summary is in. */
function AgentsScreen({
  project,
  summary,
  tabs,
  children,
}: {
  project?: ProjectResponse;
  summary?: AgentSummary;
  tabs: boolean;
  children: ReactNode;
}) {
  const t = useTranslations('agents');
  const tPages = useTranslations('projectPages');
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const view = search.view ?? 'overview';
  const { id: projectId } = Route.useParams();

  return (
    <div className="flex h-full flex-col">
      {project ? (
        <title>{tPages('agents.meta.title', { project: project.name })}</title>
      ) : null}
      <PageHeader
        title={tPages('agents.title')}
        summary={
          project && summary
            ? [
                project.name,
                t('runsCount', { count: summary.agent_runs }),
                summary.error_count > 0 ? (
                  <span className="text-sev-error-fg">
                    {t('errorsCount', { count: summary.error_count })}
                  </span>
                ) : null,
              ]
            : [<Skeleton key="summary" className="h-4 w-56" />]
        }
        tabs={
          tabs ? (
            <Tabs
              value={view}
              onValueChange={(next) =>
                navigate({
                  search: (prev) => ({
                    ...prev,
                    view:
                      next === 'overview'
                        ? undefined
                        : (next as 'traces' | 'conversations'),
                    page: undefined,
                    q: undefined,
                    sort: undefined,
                  }),
                })
              }
            >
              <TabList>
                <Tab value="overview">{t('tabs.overview')}</Tab>
                <Tab value="traces">{t('tabs.traces')}</Tab>
                <Tab value="conversations">{t('tabs.conversations')}</Tab>
              </TabList>
            </Tabs>
          ) : undefined
        }
        actions={
          project ? (
            <SavedViews
              projectId={projectId}
              page="agents"
              search={search}
              onApply={(next) =>
                navigate({ search: validateAgentsSearch(next) })
              }
            />
          ) : undefined
        }
      />
      {children}
    </div>
  );
}

/** The overview's panels, each arriving with its own read. */
function AgentsOverview({
  reads,
  summary,
  scope,
  loading = false,
}: {
  reads: AgentReads;
  /** Absent while it loads: the figures are skeleton. */
  summary?: AgentSummary;
  scope: ReactNode;
  /** Another tab or scope is on its way: the panels stay, dimmed. */
  loading?: boolean;
}) {
  const t = useTranslations('agents');
  const loadFailed = useTranslations('projectPages')('agents.loadFailed');
  const navigate = Route.useNavigate();
  const query = table.read(Route.useSearch());
  const { filters } = reads;
  // The skeleton reads nothing: an observer that unmounts mid-flight cancels
  // the loader's request, and the page would ask again.
  const idle = { enabled: !!summary, subscribed: !!summary };
  const runs = useQuery({ ...reads.runs, ...idle });
  const duration = useQuery({ ...reads.duration, ...idle });
  const models = useQuery({ ...reads.models, ...idle });
  const tools = useQuery({ ...reads.tools, ...idle });

  // A model or tool row opens the traces that used it.
  const openTraces = (key: 'model' | 'tool', value: string) => {
    const { q } = table.write({
      ...query,
      filters: [{ id: key, value: [value] }],
    });
    navigate({
      search: (prev) => ({ ...prev, view: 'traces', q, page: undefined }),
    });
  };

  return (
    <Page className={loading ? 'opacity-60' : undefined}>
      <div className="flex justify-end">{scope}</div>
      {summary ? (
        <AgentFigures summary={summary} />
      ) : (
        <Skeleton className="h-20 shrink-0 rounded-2xl" />
      )}
      <div className="grid shrink-0 gap-5 xl:grid-cols-2">
        <Panel title={t('charts.runs')} hint={t('charts.runsHint')}>
          <QueryState
            query={runs}
            placeholder={<PanelPlaceholder height={200} />}
            title={loadFailed}
          >
            {(points) => (
              <RunsChart
                points={fillBuckets(
                  points,
                  filters.intervalHours,
                  filters.periodHours,
                  (bucket) => ({ bucket, value: 0, errors: 0 }),
                )}
              />
            )}
          </QueryState>
        </Panel>
        <Panel title={t('charts.duration')} hint={t('charts.durationHint')}>
          {/* An empty bucket is a gap in the line, not a zero. */}
          <QueryState
            query={duration}
            placeholder={<PanelPlaceholder height={200} />}
            title={loadFailed}
          >
            {(points) => (
              <DurationChart
                points={fillBuckets(
                  points,
                  filters.intervalHours,
                  filters.periodHours,
                  (bucket) => ({ bucket, avg_ms: 0, p95_ms: 0 }),
                )}
              />
            )}
          </QueryState>
        </Panel>
      </div>
      <div className="grid shrink-0 gap-5 xl:grid-cols-2">
        <Panel
          flush
          title={t('modelsTable.title')}
          hint={t('modelsTable.hint')}
        >
          <QueryState
            query={models}
            placeholder={<PanelPlaceholder height={240} />}
            title={loadFailed}
          >
            {(rows) => (
              <ModelsTable
                rows={rows}
                onOpen={(model) => openTraces('model', model)}
              />
            )}
          </QueryState>
        </Panel>
        <Panel flush title={t('toolsTable.title')} hint={t('toolsTable.hint')}>
          <QueryState
            query={tools}
            placeholder={<PanelPlaceholder height={240} />}
            title={loadFailed}
          >
            {(rows) => (
              <ToolsTable
                rows={rows}
                onOpen={(tool) => openTraces('tool', tool)}
              />
            )}
          </QueryState>
        </Panel>
      </div>
    </Page>
  );
}

/** The traces tab: every run, filtered, sorted and paged through the URL. */
function AgentsTraces({
  reads,
  loading,
  scope,
  projectId,
}: {
  reads: AgentReads;
  loading: boolean;
  scope: ReactNode;
  projectId: number;
}) {
  const { data: page } = useSuspenseQuery(reads.traces);
  const { data: models } = useSuspenseQuery(reads.models);
  const { data: tools } = useSuspenseQuery(reads.tools);
  return (
    <TracesPanel
      page={page}
      models={models.map((m) => m.model)}
      tools={tools.map((m) => m.tool)}
      loading={loading}
      scope={scope}
      projectId={projectId}
    />
  );
}

/** The traces table, its controls on the live address. */
function TracesPanel({
  page,
  models,
  tools,
  loading,
  scope,
  projectId,
}: {
  /** Absent while the first page loads: the table draws skeleton rows. */
  page?: AgentsTracesResponse;
  models: string[];
  tools: string[];
  loading: boolean;
  scope: ReactNode;
  projectId: number;
}) {
  const navigate = Route.useNavigate();
  const router = useRouter();
  const query = table.read(Route.useSearch());
  return (
    <Page scroll={false}>
      <AgentTracesTable
        page={page}
        query={query}
        loading={loading}
        models={models}
        tools={tools}
        scope={scope}
        onOpen={(trace) =>
          router.push(`/projects/${projectId}/agents/${trace.trace_id}`)
        }
        onQueryChange={(updater) =>
          navigate({
            search: (prev) => ({
              ...prev,
              ...withoutPer(table.apply(updater, prev)),
            }),
          })
        }
      />
    </Page>
  );
}

/** The conversations tab: a page of them, the most recently active first. */
function AgentsConversations({
  reads,
  loading,
  scope,
  projectId,
}: {
  reads: AgentReads;
  loading: boolean;
  scope: ReactNode;
  projectId: number;
}) {
  const { data: conversations } = useSuspenseQuery(reads.conversations);
  return (
    <ConversationsPanel
      page={conversations}
      loading={loading}
      scope={scope}
      projectId={projectId}
    />
  );
}

function ConversationsPanel({
  page,
  loading,
  scope,
  projectId,
}: {
  /** Absent while the first page loads: the list draws skeleton rows. */
  page?: AgentsConversationsResponse;
  loading: boolean;
  scope: ReactNode;
  projectId: number;
}) {
  const navigate = Route.useNavigate();
  const search = Route.useSearch();
  return (
    <Page scroll={false}>
      <div className="flex justify-end">{scope}</div>
      <ConversationsList
        projectId={projectId}
        page={page}
        currentPage={searchPage(search.page) ?? 1}
        loading={loading}
        onPageChange={(next) =>
          navigate({
            search: (prev) => ({
              ...prev,
              page: next === 1 ? undefined : next,
            }),
          })
        }
      />
    </Page>
  );
}

type AgentReads = ReturnType<typeof agentReads>;
