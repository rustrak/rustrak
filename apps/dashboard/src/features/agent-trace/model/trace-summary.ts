import type { SpanResponse } from '@rustrak/client';
import { type WaterfallRow, waterfallRows } from '@/shared/lib/waterfall';
import { defaultSelectedSpanId } from './filters';

/** The header figures for one agent trace. */
export interface TraceSummary {
  /** The first agent name any span reports, or `undefined`. */
  agentName: string | null | undefined;
  /** Earliest start to latest end, in ms, or `null` if nothing is timed. */
  duration: number | null;
  /** Epoch ms of the earliest span, or `null`. */
  startedAt: number | null;
  totalTokens: number;
  toolCallCount: number;
  llmCallCount: number;
  errorCount: number;
  /** Every model the trace touched, each listed once. */
  models: string[];
}

function epochMs(iso: string | null | undefined): number | null {
  if (!iso) return null;
  const parsed = Date.parse(iso);
  return Number.isNaN(parsed) ? null : parsed;
}

/**
 * Everything the trace header states, derived from the spans in one pass of
 * the list per figure.
 *
 * Pure and out of the page on purpose: the double-counting rule below is a
 * fact about the gen_ai semantics, not about this route, and it is the kind of
 * thing that should be provable rather than commented.
 */
export function summarizeTrace(spans: SpanResponse[]): TraceSummary {
  const starts = spans
    .map((s) => epochMs(s.start_timestamp))
    .filter((v): v is number => v != null);
  const ends = spans
    .map((s) => epochMs(s.timestamp))
    .filter((v): v is number => v != null);

  const timed = starts.length > 0 && ends.length > 0;

  return {
    agentName: spans.find((s) => s.gen_ai_agent_name)?.gen_ai_agent_name,

    duration: timed ? Math.max(...ends) - Math.min(...starts) : null,
    startedAt: starts.length > 0 ? Math.min(...starts) : null,

    // Agent spans aggregate their children's usage, so including them here
    // would count the same tokens twice -- the Traces query excludes them too.
    totalTokens: spans
      .filter((s) => s.gen_ai_operation_type !== 'agent')
      .reduce((sum, s) => sum + (s.gen_ai_usage_total_tokens ?? 0), 0),

    toolCallCount: spans.filter((s) => s.gen_ai_operation_type === 'tool')
      .length,
    llmCallCount: spans.filter((s) => s.gen_ai_operation_type === 'ai_client')
      .length,
    errorCount: spans.filter((s) => s.status != null && s.status !== 'ok')
      .length,

    // Response model where the SDK reported one, request model otherwise: a
    // failed call has no response model but still names what it tried to call.
    models: [
      ...new Set(
        spans.flatMap((s) => {
          const model = s.gen_ai_response_model ?? s.gen_ai_request_model;
          return model ? [model] : [];
        }),
      ),
    ],
  };
}

export interface SelectedSpan {
  selectedSpanId: string | undefined;
  /** The URL named a span this trace does not contain. */
  requestedMissing: boolean;
}

/**
 * Which span the details panel opens on.
 *
 * Opening a trace with an empty panel wastes the whole right half of the page
 * on "select a span", so with nothing requested it falls to the first
 * generation span. Sentry defaults the same way, and the URL stays clean until
 * the reader picks one themselves.
 *
 * A requested id is honoured only if it belongs to this trace. `getSpan` is
 * scoped to the project, not the trace, so without this check a span from
 * another trace would render beside this one's waterfall -- the URL and the
 * panel describing different things. It also means the only ids that ever
 * reach the request are ones already loaded from the server.
 */
export function resolveSelectedSpan(
  spans: SpanResponse[],
  requestedSpanId: string | undefined,
): SelectedSpan {
  if (requestedSpanId == null) {
    return {
      selectedSpanId: defaultSelectedSpanId(spans),
      requestedMissing: false,
    };
  }

  const inTrace = spans.some((s) => s.id === requestedSpanId);

  return {
    selectedSpanId: inTrace ? requestedSpanId : undefined,
    requestedMissing: !inTrace,
  };
}

/**
 * The trace as waterfall rows, children linked through `parent_span_id`, each
 * named by what it ran -- the agent, the model, the tool -- since the op
 * already says what kind it is.
 */
export function waterfallSpans(spans: SpanResponse[]): WaterfallRow[] {
  return waterfallRows(
    spans.map((s) => ({
      id: s.id,
      spanId: s.span_id,
      parentSpanId: s.parent_span_id,
      startMs: epochMs(s.start_timestamp),
      endMs: epochMs(s.timestamp),
      op: s.op ?? undefined,
      description: spanName(s) ?? s.description ?? undefined,
      status: s.status ?? undefined,
      operationType: s.gen_ai_operation_type ?? undefined,
    })),
    { origin: summarizeTrace(spans).startedAt ?? 0, orphanParentId: null },
  );
}

/** What a span ran: its agent, its model or its tool. */
function spanName(span: SpanResponse): string | null {
  switch (span.gen_ai_operation_type) {
    case 'agent':
      return span.gen_ai_agent_name;
    case 'ai_client':
      return span.gen_ai_response_model ?? span.gen_ai_request_model;
    case 'tool':
      return span.gen_ai_tool_name;
    default:
      return null;
  }
}
