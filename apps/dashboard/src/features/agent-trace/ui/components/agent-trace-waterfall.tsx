import type { Span } from '@rustrak/client';
import { Link } from '@tanstack/react-router';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslations } from 'use-intl';
import { cn } from '@/shared/lib/utils';
import { barGeometry } from '@/shared/lib/waterfall-geometry';

interface AgentTraceWaterfallProps {
  spans: Span[];
  projectId: number;
  traceId: string;
  /**
   * Row id of the span whose details are open, from the `span` search param.
   *
   * Selection lives in the URL rather than in component state so the details
   * panel can be fetched on the server, and so a link to one span of one trace
   * is shareable — the same reason Sentry keeps its own span selection there.
   */
  selectedSpanId?: string;
}

interface TreeNode {
  span: Span;
  depth: number;
  children: TreeNode[];
}

interface FlatRow {
  span: Span;
  depth: number;
  hasChildren: boolean;
  collapsed: boolean;
  selfMs: number | null;
}

// gen_ai.operation.type takes precedence over the op-prefix palette below —
// it's the semantically meaningful axis for an agent trace (agent vs tool vs
// raw LLM call vs handoff), whereas `op` is often just "gen_ai.invoke_agent".
const OPERATION_TYPE_COLOR: Record<string, string> = {
  agent: 'bg-violet-500',
  tool: 'bg-amber-500',
  ai_client: 'bg-emerald-500',
  handoff: 'bg-cyan-500',
};

function opColor(span: Span): string {
  const genAiColor = span.gen_ai_operation_type
    ? OPERATION_TYPE_COLOR[span.gen_ai_operation_type]
    : undefined;
  if (genAiColor) return genAiColor;
  const o = (span.op ?? '').toLowerCase();
  if (o.startsWith('db')) return 'bg-blue-500';
  if (o.startsWith('http')) return 'bg-emerald-500';
  if (o.startsWith('resource')) return 'bg-purple-500';
  if (o.startsWith('ui') || o.includes('render')) return 'bg-orange-500';
  if (o.startsWith('cache')) return 'bg-pink-500';
  if (o.startsWith('rpc') || o.startsWith('grpc')) return 'bg-cyan-500';
  return 'bg-primary';
}

function epochMs(iso: string | null): number | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  return Number.isNaN(t) ? null : t;
}

function formatDuration(ms: number | null): string {
  if (ms == null) return '—';
  if (ms < 1) return `${(ms * 1000).toFixed(0)}µs`;
  if (ms < 1000) return `${ms.toFixed(1)}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

/**
 * Builds the span tree from `parent_span_id` links. Orphan spans (parent not
 * found in this trace) attach to a synthetic root — unlike the transaction
 * waterfall there's no single segment span guaranteed to anchor everything,
 * since a trace can span multiple standalone-span "segments".
 */
function buildTree(spans: Span[]): TreeNode[] {
  const known = new Set(
    spans.map((s) => s.span_id).filter((id): id is string => Boolean(id)),
  );
  const childrenByParent = new Map<string, Span[]>();

  for (const span of spans) {
    const key =
      span.parent_span_id && known.has(span.parent_span_id)
        ? span.parent_span_id
        : '__root__';
    const list = childrenByParent.get(key) ?? [];
    list.push(span);
    childrenByParent.set(key, list);
  }

  const visited = new Set<string>();
  const build = (parentKey: string, depth: number): TreeNode[] => {
    const children = (childrenByParent.get(parentKey) ?? []).sort(
      (a, b) =>
        (epochMs(a.start_timestamp) ?? 0) - (epochMs(b.start_timestamp) ?? 0),
    );
    const nodes: TreeNode[] = [];
    for (const span of children) {
      if (span.span_id && visited.has(span.span_id)) continue;
      if (span.span_id) visited.add(span.span_id);
      nodes.push({
        span,
        depth,
        children: span.span_id ? build(span.span_id, depth + 1) : [],
      });
    }
    return nodes;
  };

  const roots = build('__root__', 0);
  for (const span of spans) {
    if (span.span_id && !visited.has(span.span_id)) {
      roots.push({ span, depth: 0, children: [] });
    }
  }
  return roots;
}

/** Self time in ms: the `exclusive_time_ms` column, already computed server-side. */
function selfTime(node: TreeNode): number | null {
  return node.span.exclusive_time_ms ?? node.span.duration_ms;
}

function flatten(nodes: TreeNode[], collapsed: Set<string>): FlatRow[] {
  const out: FlatRow[] = [];
  const walk = (list: TreeNode[]) => {
    for (const node of list) {
      const hasChildren = node.children.length > 0;
      const isCollapsed = node.span.span_id
        ? collapsed.has(node.span.span_id)
        : false;
      out.push({
        span: node.span,
        depth: node.depth,
        hasChildren,
        collapsed: isCollapsed,
        selfMs: selfTime(node),
      });
      if (hasChildren && !isCollapsed) walk(node.children);
    }
  };
  walk(nodes);
  return out;
}

function opBreakdown(tree: TreeNode[]): { color: string; ms: number }[] {
  const byColor = new Map<string, number>();
  const walk = (nodes: TreeNode[]) => {
    for (const node of nodes) {
      const self = selfTime(node) ?? 0;
      const color = opColor(node.span);
      byColor.set(color, (byColor.get(color) ?? 0) + self);
      walk(node.children);
    }
  };
  walk(tree);
  return [...byColor.entries()]
    .flatMap(([color, ms]) => (ms > 0 ? [{ color, ms }] : []))
    .sort((a, b) => b.ms - a.ms);
}

/**
 * Waterfall for one agent trace. Unlike `SpanWaterfall` (transaction detail),
 * spans here come from `listSpans({ trace_id })` — the flat `spans` table
 * shape, which covers both standalone and transaction-embedded spans
 * uniformly and already carries precomputed `duration_ms`/`exclusive_time_ms`
 * plus the gen_ai.* columns needed for the detail panel.
 */
export function AgentTraceWaterfall({
  spans,
  projectId,
  traceId,
  selectedSpanId,
}: AgentTraceWaterfallProps) {
  const t = useTranslations('agents');
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  const tree = useMemo(() => buildTree(spans), [spans]);
  const rows = useMemo(() => flatten(tree, collapsed), [tree, collapsed]);
  const breakdown = useMemo(() => opBreakdown(tree), [tree]);

  const starts = spans
    .map((s) => epochMs(s.start_timestamp))
    .filter((v): v is number => v != null);
  const ends = spans
    .map((s) => epochMs(s.timestamp))
    .filter((v): v is number => v != null);
  const traceStart = starts.length > 0 ? Math.min(...starts) : 0;
  const traceEnd = ends.length > 0 ? Math.max(...ends) : 0;
  const total = traceEnd - traceStart;

  const breakdownTotal = breakdown.reduce((a, b) => a + b.ms, 0);

  const toggle = (id?: string | null) => {
    if (!id) return;
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  if (spans.length === 0) {
    return (
      <p className="text-sm text-muted-foreground px-1 py-4 text-center">
        {t('empty.noSpans')}
      </p>
    );
  }

  return (
    <div className="space-y-2">
      {breakdownTotal > 0 && (
        <div className="flex h-1.5 w-full overflow-hidden rounded-full">
          {breakdown.map((b) => (
            <div
              key={b.color}
              className={b.color}
              style={{ width: `${(b.ms / breakdownTotal) * 100}%` }}
              title={formatDuration(b.ms)}
            />
          ))}
        </div>
      )}

      <div className="space-y-0.5 font-mono text-xs">
        {rows.map((row, i) => (
          // `span_id` is the key wherever the span has one. The index is the
          // fallback for a span the SDK sent without an id, which nothing else
          // can distinguish.
          // react-doctor-disable-next-line react-doctor/no-array-index-as-key
          <AgentTraceRow
            key={row.span.span_id ?? `span-${i}`}
            row={row}
            projectId={projectId}
            traceId={traceId}
            traceStart={traceStart}
            total={total}
            isSelected={selectedSpanId === row.span.id}
            onToggle={toggle}
          />
        ))}
      </div>
    </div>
  );
}

interface AgentTraceRowProps {
  row: FlatRow;
  projectId: number;
  traceId: string;
  /** Epoch ms of the earliest span, the left edge of every bar's track. */
  traceStart: number;
  /** Span of the whole trace in ms, the width of that track. */
  total: number;
  isSelected: boolean;
  onToggle: (id?: string | null) => void;
}

/** One span on the shared clock, with its collapse control beside it. */
function AgentTraceRow({
  row,
  projectId,
  traceId,
  traceStart,
  total,
  isSelected,
  onToggle,
}: AgentTraceRowProps) {
  const { span, depth, hasChildren, collapsed, selfMs } = row;

  const dur = span.duration_ms;
  const { offsetPct, widthPct } = barGeometry(
    epochMs(span.start_timestamp),
    dur,
    traceStart,
    total,
  );

  return (
    <div
      className={cn(
        'flex w-full items-center gap-1 rounded-md pr-2 hover:bg-muted/40',
        isSelected && 'bg-muted/50',
      )}
    >
      {/* The collapse control is a sibling of the row link, not a child of it.
          It used to be nested inside the selectable row, which meant one
          interactive element inside another and a stopPropagation to keep them
          apart; as siblings both are plain, valid, and independently reachable
          by keyboard. */}
      <CollapseControl
        depth={depth}
        hasChildren={hasChildren}
        collapsed={collapsed}
        onToggle={() => onToggle(span.span_id)}
      />

      {/* Selecting toggles: clicking the open row closes the panel. The list
          keeps its scroll, or every selection would jump back to the top. */}
      <Link
        to="/projects/$id/agents/$traceId"
        params={{ id: projectId, traceId }}
        search={isSelected ? {} : { span: span.id }}
        resetScroll={false}
        aria-current={isSelected ? 'true' : undefined}
        className="flex min-w-0 flex-1 items-center gap-3 py-1 text-left"
      >
        <SpanLabel span={span} />
        <div className="relative h-4 flex-1">
          <div
            className={cn('absolute inset-y-0 rounded-sm', opColor(span))}
            style={{ left: `${offsetPct}%`, width: `${widthPct}%` }}
          />
        </div>
        <span
          className="w-16 text-right tabular-nums text-muted-foreground"
          title={selfMs != null ? formatDuration(selfMs) : undefined}
        >
          {formatDuration(dur)}
        </span>
      </Link>
    </div>
  );
}

/**
 * Indents the row to its depth and, for a span with children, holds the
 * control that folds them away.
 */
function CollapseControl({
  depth,
  hasChildren,
  collapsed,
  onToggle,
}: {
  depth: number;
  hasChildren: boolean;
  collapsed: boolean;
  onToggle: () => void;
}) {
  const t = useTranslations('agents');
  const Chevron = collapsed ? ChevronRight : ChevronDown;

  return (
    <div
      className="flex shrink-0 items-center"
      style={{ paddingLeft: `${Math.min(depth, 8) * 12 + 8}px` }}
    >
      {hasChildren ? (
        <button
          type="button"
          aria-label={
            collapsed ? t('waterfall.expand') : t('waterfall.collapse')
          }
          onClick={onToggle}
          className="text-muted-foreground hover:text-foreground"
        >
          <Chevron className="size-3" />
        </button>
      ) : (
        <span className="inline-block size-3" />
      )}
    </div>
  );
}

/** The span's operation, the name it goes by, and its status when it failed. */
function SpanLabel({ span }: { span: Span }) {
  const t = useTranslations('agents');
  const failed = span.status && span.status !== 'ok';
  const label =
    span.gen_ai_agent_name ||
    span.gen_ai_tool_name ||
    span.gen_ai_response_model ||
    span.description;

  return (
    <div className="flex w-[38%] min-w-0 items-center gap-1">
      <span
        className={cn(
          'shrink-0 rounded px-1 py-px text-[10px] font-medium text-white',
          opColor(span),
        )}
      >
        {span.gen_ai_operation_type || span.op || t('waterfall.spanFallback')}
      </span>
      <span className="truncate text-muted-foreground">{label || '—'}</span>
      {failed && (
        <span className="shrink-0 rounded bg-destructive/15 px-1 text-[10px] text-destructive">
          {span.status}
        </span>
      )}
    </div>
  );
}
