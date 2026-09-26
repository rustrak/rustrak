/**
 * Reads for the agent-monitoring pages.
 *
 * No `mutations.ts` beside this: nothing in the product writes an agent trace,
 * they arrive through ingestion. A slice with only one half of its `api`
 * segment is the expected shape, not an omission.
 */

import type {
  AgentBreakdownOptions,
  AgentDurationPoint,
  AgentModelRow,
  AgentSummary,
  AgentTimeseriesOptions,
  AgentTimeseriesPoint,
  AgentToolRow,
  AgentTraceSummary,
  AgentTracesOptions,
  GenAiBreakdownRow,
  ListSpansOptions,
  OffsetPaginatedResponse,
  Result,
  RustrakError,
  Span,
  SpanDetail,
} from '@rustrak/client';
import { Ok } from '@rustrak/client';
import { queryOptions } from '@tanstack/react-query';
import type { AgentDashboardFilters } from '@/features/agent-trace/model/filters';
import { scope } from '@/shared/api/query-client';
import { createClient } from '@/shared/api/rustrak';
import { loadAll } from '@/shared/lib/results';

export async function listSpans(
  projectId: number,
  options?: ListSpansOptions,
): Promise<Result<OffsetPaginatedResponse<Span>, RustrakError>> {
  const client = await createClient();
  return client.spans.list(projectId, options);
}

/**
 * One span with its raw `gen_ai.*` attribute bag — the prompts, responses and
 * tool arguments the list deliberately leaves out.
 *
 * Fetched per selected span rather than for the whole trace: the server never
 * trims `spans.data`, so a trace's worth of prompts would dwarf the waterfall
 * they are drawn from.
 */
export async function getSpan(
  projectId: number,
  spanId: string,
): Promise<Result<SpanDetail, RustrakError>> {
  const client = await createClient();
  return client.spans.get(projectId, spanId);
}

export async function getAgentRuns(
  projectId: number,
  options?: AgentTimeseriesOptions,
): Promise<Result<AgentTimeseriesPoint[], RustrakError>> {
  const client = await createClient();
  return client.agents.getRuns(projectId, options);
}

export async function getAgentDuration(
  projectId: number,
  options?: AgentTimeseriesOptions,
): Promise<Result<AgentDurationPoint[], RustrakError>> {
  const client = await createClient();
  return client.agents.getDuration(projectId, options);
}

export async function getAgentModelsByCalls(
  projectId: number,
  options?: AgentBreakdownOptions,
): Promise<Result<GenAiBreakdownRow[], RustrakError>> {
  const client = await createClient();
  return client.agents.getModelsByCalls(projectId, options);
}

export async function getAgentModelsByTokens(
  projectId: number,
  options?: AgentBreakdownOptions,
): Promise<Result<GenAiBreakdownRow[], RustrakError>> {
  const client = await createClient();
  return client.agents.getModelsByTokens(projectId, options);
}

export async function getAgentTools(
  projectId: number,
  options?: AgentBreakdownOptions,
): Promise<Result<GenAiBreakdownRow[], RustrakError>> {
  const client = await createClient();
  return client.agents.getTools(projectId, options);
}

export async function getAgentTraces(
  projectId: number,
  options?: AgentTracesOptions,
): Promise<Result<OffsetPaginatedResponse<AgentTraceSummary>, RustrakError>> {
  const client = await createClient();
  return client.agents.getTraces(projectId, options);
}

export async function getAgentSummary(
  projectId: number,
  options?: AgentBreakdownOptions,
): Promise<Result<AgentSummary, RustrakError>> {
  const client = await createClient();
  return client.agents.getSummary(projectId, options);
}

export async function getAgentModelsTable(
  projectId: number,
  options?: AgentBreakdownOptions,
): Promise<Result<AgentModelRow[], RustrakError>> {
  const client = await createClient();
  return client.agents.getModelsTable(projectId, options);
}

export async function getAgentToolsTable(
  projectId: number,
  options?: AgentBreakdownOptions,
): Promise<Result<AgentToolRow[], RustrakError>> {
  const client = await createClient();
  return client.agents.getToolsTable(projectId, options);
}

export async function getAgentEnvironments(
  projectId: number,
): Promise<Result<string[], RustrakError>> {
  const client = await createClient();
  return client.agents.getEnvironments(projectId);
}

const TRACE_SPANS_PER_PAGE = 100;

/**
 * Every span of one trace. The totals and the waterfall are only correct over
 * the whole trace, so the pages past the first are pulled too.
 */
async function listTraceSpans(
  projectId: number,
  traceId: string,
): Promise<Result<Span[], RustrakError>> {
  const page = (n: number) =>
    listSpans(projectId, {
      trace_id: traceId,
      per_page: TRACE_SPANS_PER_PAGE,
      page: n,
    });

  const first = await page(1);
  if (!first.success) return first;

  const rest = await Promise.all(
    Array.from({ length: first.data.total_pages - 1 }, (_, i) => page(i + 2)),
  );

  const spans = [...first.data.items];
  for (const next of rest) {
    // A missing page is not an empty page: a partial set would draw a
    // plausible and wrong waterfall.
    if (!next.success) return next;
    spans.push(...next.data.items);
  }
  return Ok(spans);
}

export const agentQueries = {
  runs: (projectId: number, options?: AgentTimeseriesOptions) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'agents', 'runs', options],
      queryFn: () => getAgentRuns(projectId, options),
    }),
  duration: (projectId: number, options?: AgentTimeseriesOptions) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'agents', 'duration', options],
      queryFn: () => getAgentDuration(projectId, options),
    }),
  modelsByCalls: (projectId: number, options?: AgentBreakdownOptions) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'agents', 'model-calls', options],
      queryFn: () => getAgentModelsByCalls(projectId, options),
    }),
  modelsByTokens: (projectId: number, options?: AgentBreakdownOptions) =>
    queryOptions({
      queryKey: [
        ...scope.project(projectId),
        'agents',
        'model-tokens',
        options,
      ],
      queryFn: () => getAgentModelsByTokens(projectId, options),
    }),
  tools: (projectId: number, options?: AgentBreakdownOptions) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'agents', 'tools', options],
      queryFn: () => getAgentTools(projectId, options),
    }),
  traces: (projectId: number, options?: AgentTracesOptions) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'agents', 'traces', options],
      queryFn: () => getAgentTraces(projectId, options),
    }),
  /**
   * Everything the agents dashboard draws, as one read. The page renders all
   * of it or its failure, so the ten requests are one cache entry.
   */
  dashboard: (
    projectId: number,
    filters: AgentDashboardFilters,
    page: number,
  ) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'agents', filters, page],
      queryFn: () => {
        const series = {
          period_hours: filters.periodHours,
          interval_hours: filters.intervalHours,
          environment: filters.environment,
        };
        const breakdown = {
          period_hours: filters.periodHours,
          environment: filters.environment,
        };
        return loadAll([
          getAgentRuns(projectId, series),
          getAgentDuration(projectId, series),
          getAgentModelsByCalls(projectId, breakdown),
          getAgentModelsByTokens(projectId, breakdown),
          getAgentTools(projectId, breakdown),
          getAgentTraces(projectId, {
            page,
            per_page: 20,
            period_hours: filters.periodHours,
            environment: filters.environment,
          }),
          getAgentSummary(projectId, breakdown),
          getAgentModelsTable(projectId, breakdown),
          getAgentToolsTable(projectId, breakdown),
          // Not filtered by the current environment: the picker has to keep
          // offering the option you would switch back to.
          getAgentEnvironments(projectId),
        ]);
      },
    }),
  traceSpans: (projectId: number, traceId: string) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'traces', traceId],
      queryFn: () => listTraceSpans(projectId, traceId),
    }),
  span: (projectId: number, spanId: string) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'spans', spanId],
      queryFn: () => getSpan(projectId, spanId),
    }),
};
