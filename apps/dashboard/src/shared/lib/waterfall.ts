/** A span on the waterfall's clock, before it is placed. */
export interface ClockSpan {
  /** The row's id. */
  id: string;
  /** The protocol `span_id` children point at. */
  spanId?: string | null;
  parentSpanId?: string | null;
  startMs: number | null;
  endMs: number | null;
  op?: string;
  description?: string;
  status?: string;
  operationType?: string;
  /** The SDK's `exclusive_time`, when it sent one. */
  exclusiveMs?: number;
}

/** One row of the trace waterfall, in the shape `@rustrak/ui`'s takes. */
export interface WaterfallRow {
  id: string;
  parentId: string | null;
  op?: string;
  description?: string;
  startMs: number;
  endMs: number;
  status?: string;
  operationType?: string;
  /** Missing a timestamp: drawn as a point and labelled, not timed. */
  unfinished: boolean;
  /**
   * Time spent in the span itself: the SDK's `exclusive_time` when it sent
   * one, else its duration less its children's, overlaps counted once.
   */
  selfMs: number;
}

/**
 * Spans as waterfall rows. Every span is kept: one missing a timestamp is an
 * unfinished row with no width, at the one time it has, else its parent's
 * start, else `origin`. A parent that was never sent becomes `orphanParentId`.
 */
export function waterfallRows(
  spans: ClockSpan[],
  options: { origin: number; orphanParentId: string | null },
): WaterfallRow[] {
  const rowOf = new Map<string, string>();
  const startOf = new Map<string, number>();
  for (const span of spans) {
    if (!span.spanId) continue;
    rowOf.set(span.spanId, span.id);
    if (span.startMs !== null) startOf.set(span.spanId, span.startMs);
  }

  const rows = spans.map((span) => {
    const at =
      span.startMs ??
      span.endMs ??
      (span.parentSpanId ? startOf.get(span.parentSpanId) : undefined) ??
      options.origin;
    const unfinished = span.startMs === null || span.endMs === null;
    return {
      id: span.id,
      parentId:
        (span.parentSpanId && rowOf.get(span.parentSpanId)) ||
        (options.orphanParentId === span.id ? null : options.orphanParentId),
      op: span.op,
      description: span.description,
      startMs: at,
      endMs: unfinished ? at : (span.endMs as number),
      status: span.status,
      operationType: span.operationType,
      unfinished,
      exclusiveMs: span.exclusiveMs,
    };
  });

  const childrenOf = Map.groupBy(rows, (row) => row.parentId);

  return rows.map(({ exclusiveMs, ...row }): WaterfallRow => {
    let covered = 0;
    let reached = row.startMs;
    const children = [...(childrenOf.get(row.id) ?? [])].sort(
      (a, b) => a.startMs - b.startMs,
    );
    for (const child of children) {
      const start = Math.max(child.startMs, reached);
      const end = Math.min(child.endMs, row.endMs);
      if (end > start) covered += end - start;
      reached = Math.max(reached, end);
    }
    const own = row.endMs - row.startMs;
    return { ...row, selfMs: exclusiveMs ?? Math.max(0, own - covered) };
  });
}
