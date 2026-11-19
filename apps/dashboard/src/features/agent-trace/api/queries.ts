/**
 * Reads for the agent-monitoring pages.
 *
 * No `mutations.ts` beside this: nothing in the product writes an agent trace,
 * they arrive through ingestion. A slice with only one half of its `api`
 * segment is the expected shape, not an omission.
 */
import type {
  AgentsConversationsData,
  AgentsRunsData,
  AgentsSummaryData,
  AgentsTracesData,
  SpanResponse,
} from '@rustrak/client';
import { queryOptions } from '@tanstack/react-query';
import { projectScope } from '@/shared/api/keys';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';
import { allPages } from '../model/pages';

type AgentWindow = NonNullable<AgentsSummaryData['query']>;
type AgentTimeseriesQuery = NonNullable<AgentsRunsData['query']>;
type AgentTracesQuery = NonNullable<AgentsTracesData['query']>;
type AgentConversationsQuery = NonNullable<AgentsConversationsData['query']>;

/** Spans per page of a trace; a trace with more is read page by page. */
const TRACE_PAGE = 100;

export const agentQueries = {
  all: (projectId: number) => [...projectScope(projectId), 'agents'] as const,
  summary: (projectId: number, query: AgentWindow) =>
    queryOptions({
      queryKey: [...agentQueries.all(projectId), 'summary', query],
      queryFn: ({ signal }) =>
        orThrow(
          api().agents.summary({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
  runs: (projectId: number, query: AgentTimeseriesQuery) =>
    queryOptions({
      queryKey: [...agentQueries.all(projectId), 'runs', query],
      queryFn: ({ signal }) =>
        orThrow(
          api().agents.runs({ path: { project_id: projectId }, query, signal }),
        ),
    }),
  duration: (projectId: number, query: AgentTimeseriesQuery) =>
    queryOptions({
      queryKey: [...agentQueries.all(projectId), 'duration', query],
      queryFn: ({ signal }) =>
        orThrow(
          api().agents.duration({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
  models: (projectId: number, query: AgentWindow) =>
    queryOptions({
      queryKey: [...agentQueries.all(projectId), 'models', query],
      queryFn: ({ signal }) =>
        orThrow(
          api().agents.modelsTable({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
  tools: (projectId: number, query: AgentWindow) =>
    queryOptions({
      queryKey: [...agentQueries.all(projectId), 'tools', query],
      queryFn: ({ signal }) =>
        orThrow(
          api().agents.toolsTable({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
  traces: (projectId: number, query: AgentTracesQuery) =>
    queryOptions({
      queryKey: [...agentQueries.all(projectId), 'traces', query],
      queryFn: ({ signal }) =>
        orThrow(
          api().agents.traces({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
  /** Every environment with an AI span, whatever the window. */
  environments: (projectId: number) =>
    queryOptions({
      queryKey: [...agentQueries.all(projectId), 'environments'],
      queryFn: ({ signal }) =>
        orThrow(
          api().agents.environments({
            path: { project_id: projectId },
            signal,
          }),
        ),
    }),
  conversations: (projectId: number, query: AgentConversationsQuery) =>
    queryOptions({
      queryKey: [...agentQueries.all(projectId), 'conversations', query],
      queryFn: ({ signal }) =>
        orThrow(
          api().agents.conversations({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
  conversation: (projectId: number, conversationId: string) =>
    queryOptions({
      queryKey: [
        ...agentQueries.all(projectId),
        'conversation',
        conversationId,
      ],
      queryFn: ({ signal }) =>
        orThrow(
          api().agents.conversation({
            path: { project_id: projectId, conversation_id: conversationId },
            signal,
          }),
        ),
    }),
  /**
   * Every span of a trace. The totals and the waterfall are only correct over
   * the whole trace, so the pages past the first are read too, and a page
   * that fails fails the read: a partial set would draw a plausible and wrong
   * picture.
   */
  trace: (projectId: number, traceId: string) =>
    queryOptions({
      queryKey: [...agentQueries.all(projectId), 'trace', traceId],
      queryFn: async ({ signal }): Promise<SpanResponse[]> => {
        const page = (n: number) =>
          orThrow(
            api().spans.list({
              path: { project_id: projectId },
              query: { trace_id: traceId, per_page: TRACE_PAGE, page: n },
              signal,
            }),
          );
        return allPages(page);
      },
    }),
  /**
   * One span with its raw `gen_ai.*` attribute bag — the prompts, responses
   * and tool arguments the list deliberately leaves out. Read per selected
   * span: the server never trims `spans.data`, so a trace's worth of prompts
   * would dwarf the waterfall they are drawn from.
   */
  span: (projectId: number, spanId: string) =>
    queryOptions({
      queryKey: [...agentQueries.all(projectId), 'span', spanId],
      queryFn: ({ signal }) =>
        orThrow(
          api().spans.get({
            path: { project_id: projectId, span_id: spanId },
            signal,
          }),
        ),
    }),
};
