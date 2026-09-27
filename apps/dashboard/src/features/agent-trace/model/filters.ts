/**
 * The window and environment the agents dashboard is read through.
 *
 * Portable core: no React, no Next. The page parses search params through
 * here so every widget on it agrees on one window, and so an unparseable
 * param degrades to the default rather than reaching a query.
 */

/** Selectable windows, as the labels a reader sees. */
export const AGENT_PERIODS = ['1h', '24h', '7d', '30d'] as const;

type AgentPeriod = (typeof AGENT_PERIODS)[number];

const PERIOD_HOURS: Record<AgentPeriod, number> = {
  '1h': 1,
  '24h': 24,
  '7d': 24 * 7,
  '30d': 24 * 30,
};

/**
 * Bucket width for a window, chosen so every chart lands between roughly 24
 * and 60 points. A 30-day window bucketed hourly is 720 points on a chart a
 * few hundred pixels wide, which is noise rather than detail.
 */
const PERIOD_INTERVAL_HOURS: Record<AgentPeriod, number> = {
  '1h': 1,
  '24h': 1,
  '7d': 6,
  '30d': 24,
};

function isAgentPeriod(value: string | undefined): value is AgentPeriod {
  return value != null && (AGENT_PERIODS as readonly string[]).includes(value);
}

export interface AgentDashboardFilters {
  /** `undefined` means all time — the same thing the server means by it. */
  period?: AgentPeriod;
  periodHours?: number;
  intervalHours: number;
  environment?: string;
}

/**
 * Resolves the dashboard's filters from raw search params.
 *
 * An unrecognized period falls back to all-time rather than throwing: a
 * hand-edited or stale URL should show the reader something, and showing more
 * data than asked for is the safe direction to fail in.
 */
export function resolveAgentFilters(params: {
  period?: string;
  environment?: string;
}): AgentDashboardFilters {
  const period = isAgentPeriod(params.period) ? params.period : undefined;
  const environment =
    params.environment != null && params.environment !== ''
      ? params.environment
      : undefined;

  return {
    period,
    periodHours: period ? PERIOD_HOURS[period] : undefined,
    // All-time still needs a bucket width, and it is the widest window there
    // is, so it takes the widest interval.
    intervalHours: period ? PERIOD_INTERVAL_HOURS[period] : 24,
    environment,
  };
}

/**
 * The dashboard's search with one filter changed.
 *
 * Omitting a filter rather than writing an empty value keeps the URL honest:
 * `?period=7d` and `?period=7d&environment=` mean the same thing, and only
 * one of them should exist.
 */
export function agentDashboardSearch(
  current: { period?: string; environment?: string },
  change: { period?: string | null; environment?: string | null },
): { period?: string; environment?: string } {
  const period = change.period !== undefined ? change.period : current.period;
  const environment =
    change.environment !== undefined ? change.environment : current.environment;

  return {
    ...(period ? { period } : {}),
    ...(environment ? { environment } : {}),
  };
}

/**
 * The span whose details open when a trace is loaded with none selected.
 *
 * The first LLM call, falling back to the first AI span, then to the first
 * span at all — Sentry's own default, and for the same reason: an agent root
 * mostly restates the header, while the first generation is where the prompt
 * and the answer are.
 */
/**
 * A sortable instant for a span's start.
 *
 * A missing or unparseable timestamp sorts last rather than yielding `NaN`: a
 * comparator that returns `NaN` leaves the order implementation-defined, which
 * would make the default selection depend on the order rows arrived in.
 */
function sortableStart(value: string | null): number {
  if (value == null) return Number.POSITIVE_INFINITY;
  const parsed = Date.parse(value);
  return Number.isNaN(parsed) ? Number.POSITIVE_INFINITY : parsed;
}

export function defaultSelectedSpanId(
  spans: {
    id: string;
    gen_ai_operation_type: string | null;
    start_timestamp: string | null;
  }[],
): string | undefined {
  if (spans.length === 0) return undefined;

  const inOrder = [...spans].sort(
    (a, b) =>
      sortableStart(a.start_timestamp) - sortableStart(b.start_timestamp),
  );

  return (
    inOrder.find((s) => s.gen_ai_operation_type === 'ai_client')?.id ??
    inOrder.find((s) => s.gen_ai_operation_type != null)?.id ??
    inOrder[0]?.id
  );
}
