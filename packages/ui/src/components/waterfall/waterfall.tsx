import {
  type CSSProperties,
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent,
  type ReactNode,
  useMemo,
  useRef,
  useState,
} from 'react';
import { focusRingInset } from '../../lib/focus';
import { interactiveTransition } from '../../lib/motion';
import { tv } from '../../lib/tv';
import {
  ChevronDownIcon,
  ChevronRightIcon,
  EmptyIcon,
} from '../icon/icon-catalog';
import { Tag } from '../tag/tag';
import { Text } from '../text/text';
import { formatSpanDuration } from './format';

/**
 * The request, drawn: every span a bar on one shared clock.
 *
 * The reading it must give up in one glance is *where the time went and what
 * broke*. The shape follows the best tracers, not any one mock:
 *
 *   - two columns split by a draggable divider -- the tree names, the
 *     timeline measures -- because a fixed gutter is always wrong for
 *     somebody's span names;
 *   - the duration rides beside its own bar, outside until the bar is wide
 *     enough to hold it, so the number never has to be hunted across the
 *     row;
 *   - the chevron is a pill that carries the descendant count: what a fold
 *     hides is written on the fold;
 *   - runs of identical leaf siblings collapse into one autogrouped row --
 *     an N+1 of forty queries is one line saying ×40, not forty lines
 *     saying nothing;
 *   - a gap of unaccounted time between siblings can surface as a hatched
 *     "missing instrumentation" row, because time nobody claimed is a
 *     finding too.
 *
 * Colour is spent by *kind* -- `db` in chart-1, `http` in quiet grey, the
 * rest as our own code in chart-3 -- plus the error, which is the only red
 * on the page: bar, duration and row tint at once. Not one hue per op: the
 * gutter already names the op, and the question a waterfall answers is "is
 * the time in the database, on the wire, or in us".
 *
 * An AI trace asks a different question -- "is the time in the agent, the
 * model or a tool" -- so `gen_ai.*` spans get the four kinds every agent
 * tracer draws: agent, LLM call, tool call and handoff.
 */
const waterfall = tv({
  slots: {
    /*
     * `@container`: the layout answers to the box it lives in, not to the
     * viewport. Past ~670 px the trace is two columns with a draggable
     * divider; below, each row stacks -- name line above, bar below, full
     * width -- because a 120 px gutter is not a smaller version of the
     * reading, it is no reading.
     */
    root: '@container relative flex min-w-0 flex-col font-mono',
    /*
     * The ruler: what a pixel is worth. It scrolls with the trace on
     * purpose -- stuck to the viewport it escapes whatever card the app
     * drew the trace in, opaque corners and all. When the app gives the
     * trace its own scroll box, that is the right place to pin a copy.
     */
    ruler: [
      'flex h-6.5 shrink-0 items-center',
      'border-border-divider border-b ps-4 pe-4',
    ],
    rulerTrack: [
      'flex min-w-0 flex-1 justify-between',
      'text-fg-ghost text-mono-sm',
    ],
    /*
     * The divider between names and time. A real control: it drags, it
     * takes the keyboard, and it clamps -- neither column may vanish.
     */
    /* The rows and the divider that resizes them; the legend sits below,
       so the divider ends where the rows do. */
    body: 'relative',
    divider: [
      'absolute inset-y-0 z-20 w-1.5 -translate-x-1/2 cursor-ew-resize',
      '@max-2xl:hidden',
      'outline-none',
      'after:absolute after:inset-y-0 after:start-1/2 after:w-px',
      'after:bg-border-divider',
      'hover:after:bg-border-strong focus-visible:after:bg-border-brand',
    ],
    row: [
      'group/row relative flex min-h-9 w-full shrink-0 flex-wrap items-center',
      'ps-4 pe-4',
      '@max-2xl:h-auto @max-2xl:py-1.5',
      'border-border-divider border-b text-start',
      interactiveTransition,
      'hover:bg-surface-hover',
      'cursor-pointer outline-none',
      focusRingInset,
      'aria-selected:bg-surface-raised',
      // The broken span is the row the eye lands on before reading.
      'data-[failed=true]:bg-sev-error-surface/40',
    ],
    gutter: [
      'flex h-9 min-w-0 shrink-0 items-center gap-2 pe-3',
      'w-(--wf-gutter)',
      '@max-2xl:h-6 @max-2xl:w-full @max-2xl:pe-0',
    ],
    /** One indent step; carries its ancestor's guide line when it has one. */
    guide: 'relative h-full w-4 shrink-0',
    guideLine: 'absolute inset-y-0 start-1.75 w-px bg-border-divider',
    /*
     * The fold and its size in one pill: a chevron that also says how many
     * rows it is holding shut.
     */
    pill: [
      'flex h-4.5 shrink-0 items-center gap-0.5 rounded-pill',
      'bg-surface-chip ps-0.5 pe-1.5 text-fg-subtle text-mono-sm',
      interactiveTransition,
      'hover:bg-surface-selected hover:text-fg',
    ],
    pillGap: 'w-4.5 shrink-0',
    op: 'shrink-0 whitespace-nowrap font-medium text-fg-secondary text-mono-sm',
    description: 'min-w-0 truncate text-fg-ghost text-mono-sm',
    track: ['relative h-9 min-w-0 flex-1', '@max-2xl:h-6 @max-2xl:min-w-full'],
    gridline: 'absolute inset-y-0 w-px bg-border-divider/50',
    bar: [
      'absolute top-1/2 h-4 -translate-y-1/2 rounded-2xs',
      interactiveTransition,
      'group-hover/row:brightness-125',
    ],
    /*
     * The figure rides its bar: outside its right end while there is room,
     * stepping inside once the bar is wide enough to hold it. It never sits
     * in a far column the eye has to travel to.
     */
    duration: [
      'absolute top-1/2 flex h-4 -translate-y-1/2 items-center whitespace-nowrap',
      'text-fg-subtle text-mono-sm tabular-nums',
      'data-[failed=true]:text-sev-error-fg',
      'data-[inside=true]:text-fg data-[inside=true]:font-medium',
    ],
    /* Unclaimed time, in the established language for it: hatching. */
    gapBar: [
      'absolute top-1/2 h-4 -translate-y-1/2 rounded-2xs',
      'bg-[repeating-linear-gradient(135deg,var(--border-raised)_0px,var(--border-raised)_3px,transparent_3px,transparent_6px)]',
    ],
    gapLabel: 'text-fg-ghost text-mono-sm italic',
    /*
     * The selected span's detail, opened inside the trace under its row:
     * what was clicked and what answers stay in one scroll. The app
     * composes the content; the component owns the seam.
     */
    detail: [
      // `basis-full min-w-0`: a flex child's min-width is its content, and
      // a long attribute value would otherwise widen the whole trace. What
      // does not fit scrolls here, inside its own box.
      'min-w-0 basis-full overflow-x-auto',
      'cursor-default border-border-divider border-t bg-panel',
      'mt-1.5 px-3 py-3 font-sans',
      '@max-2xl:px-1',
    ],
    legend: [
      'flex shrink-0 flex-wrap items-center gap-4 px-4 py-3',
      'text-fg-subtle text-mono-sm',
    ],
    legendItem: 'inline-flex items-center gap-1.5',
    legendSwatch: 'size-2 rounded-2xs',
    /* A trace with no spans: the figure says so in words, in font-sans. */
    empty: [
      'flex flex-col items-center justify-center gap-3',
      'px-6 py-16 text-center font-sans',
    ],
  },
});

const styles = waterfall();

/**
 * One span, in the units the caller already has: milliseconds relative to
 * anything. The component only reads differences.
 */
export interface WaterfallSpan {
  id: string;
  parentId?: string | null;
  /** The operation: `db.query`, `http.server`, `cache.get`. */
  op?: string;
  /** What it did it to: the statement, the URL, the key. */
  description?: string;
  startMs: number;
  endMs: number;
  /** Anything other than `ok` or absent reads as failed. */
  status?: string;
  /** Never sent its end: drawn at its start and labelled, not timed. */
  unfinished?: boolean;
  /**
   * An AI span's kind as the server classified it (`agent`, `ai_client`,
   * `tool`, `handoff`), which knows more `op` spellings than a prefix does:
   * `ai.toolCall` is a tool. Read before `op` when present.
   */
  operationType?: string;
}

/**
 * Where a span ends on the clock. One that never ended has no end to draw --
 * its `endMs` is whatever the SDK sent -- so it ends where it started: it
 * cannot stretch the ruler, claim time from a gap, or draw a length.
 */
function endOf(span: WaterfallSpan): number {
  return span.unfinished ? span.startMs : span.endMs;
}

/** The kinds time divides into, plus the one that broke. */
type SpanKind =
  | 'db'
  | 'http'
  | 'internal'
  | 'agent'
  | 'llm'
  | 'tool'
  | 'handoff'
  | 'error';

/*
 * A static map, never `bg-${kind}`: Tailwind extracts class names from the
 * source text, so a composed name is a rule that is silently not generated.
 */
const KIND_BAR: Record<SpanKind, string> = {
  db: 'bg-chart-1',
  http: 'bg-border-control',
  internal: 'bg-chart-3',
  agent: 'bg-chart-1',
  llm: 'bg-chart-4',
  tool: 'bg-chart-5',
  handoff: 'bg-fg-ghost',
  error: 'bg-sev-error',
};

const OPERATION_KIND: Record<string, SpanKind> = {
  agent: 'agent',
  ai_client: 'llm',
  tool: 'tool',
  handoff: 'handoff',
};

function spanKind(span: WaterfallSpan): SpanKind {
  if (span.status && span.status !== 'ok') return 'error';
  const classified = span.operationType && OPERATION_KIND[span.operationType];
  if (classified) return classified;
  const op = (span.op ?? '').toLowerCase();
  if (op.startsWith('gen_ai.') || op.startsWith('ai.')) return aiKind(op);
  if (op.startsWith('db') || op.startsWith('cache')) return 'db';
  if (op.startsWith('http') || op.startsWith('resource')) return 'http';
  return 'internal';
}

/** An AI span the server did not classify, read from its `op` alone. */
function aiKind(op: string): SpanKind {
  if (op.includes('invoke_agent')) return 'agent';
  if (op.includes('execute_tool')) return 'tool';
  if (op.includes('handoff')) return 'handoff';
  return 'llm';
}

/* --- The tree ------------------------------------------------------------- */

interface TreeNode {
  span: WaterfallSpan;
  children: TreeNode[];
}

/**
 * Built from `parentId` links. An orphan -- parent never arrived, or the
 * link is cyclic -- attaches to the root rather than disappearing: a
 * dropped span is exactly the one someone is looking for.
 */
function buildTree(spans: WaterfallSpan[]): TreeNode[] {
  const known = new Set(spans.map((span) => span.id));
  const byParent = new Map<string, WaterfallSpan[]>();

  for (const span of spans) {
    const parent =
      span.parentId && known.has(span.parentId) && span.parentId !== span.id
        ? span.parentId
        : '__root__';
    const list = byParent.get(parent) ?? [];
    list.push(span);
    byParent.set(parent, list);
  }

  const visited = new Set<string>();
  const build = (parent: string): TreeNode[] => {
    const children = (byParent.get(parent) ?? []).sort(
      (a, b) => a.startMs - b.startMs,
    );
    const nodes: TreeNode[] = [];
    for (const span of children) {
      if (visited.has(span.id)) continue;
      visited.add(span.id);
      nodes.push({ span, children: build(span.id) });
    }
    return nodes;
  };

  const roots = build('__root__');
  for (const span of spans) {
    if (!visited.has(span.id)) roots.push({ span, children: [] });
  }
  return roots;
}

function descendants(node: TreeNode): number {
  return node.children.reduce((acc, c) => acc + 1 + descendants(c), 0);
}

/** How many identical leaf siblings in a row become one line. */
const AUTOGROUP_MIN = 5;
/** Unclaimed time between siblings worth its own row, in ms. */
const GAP_MIN_MS = 100;

type Row =
  | {
      kind: 'span';
      id: string;
      span: WaterfallSpan;
      depth: number;
      guides: boolean[];
      hasChildren: boolean;
      collapsed: boolean;
      count: number;
    }
  | {
      kind: 'group';
      id: string;
      op: string;
      description?: string;
      spans: WaterfallSpan[];
      depth: number;
      guides: boolean[];
      failed: boolean;
    }
  | {
      kind: 'gap';
      id: string;
      depth: number;
      guides: boolean[];
      startMs: number;
      endMs: number;
    };

interface FlattenState {
  collapsed: Set<string>;
  openGroups: Set<string>;
  showGaps: boolean;
}

/**
 * The coverage frontier, which only ever moves forward.
 *
 * Siblings overlap -- a slow query still running while a fast one starts
 * and finishes inside it -- so taking the last unit's end as the frontier
 * would walk it backwards and hatch time the slow span demonstrably
 * covered. The gap is only what *nobody* claimed.
 */
function advance(frontier: number | null, ...ends: number[]): number {
  return Math.max(frontier ?? Number.NEGATIVE_INFINITY, ...ends);
}

/**
 * The tree, walked into rows, with the two transforms real traces need:
 *
 * Runs of `AUTOGROUP_MIN`+ consecutive *leaf* siblings sharing op and
 * description fold into one autogrouped row (the N+1 case), reopened from
 * its pill. And, when asked, a gap of `GAP_MIN_MS`+ between siblings
 * becomes a hatched row: time inside the parent that no child claims.
 *
 * `guides` says, per ancestor level, whether that ancestor still has
 * siblings below -- which is exactly where a vertical guide line belongs.
 */
/** One sibling, or a run of them folded into an autogroup. */
type SiblingUnit =
  | { unit: 'node'; node: TreeNode }
  | { unit: 'run'; nodes: TreeNode[] };

/**
 * Splits a sibling list into single nodes and autogroupable runs.
 *
 * A run is `AUTOGROUP_MIN`+ consecutive siblings that are all leaves and all
 * share an op and description: the N+1 shape. A node with children never joins
 * one, because folding it would hide the subtree underneath it.
 */
function partitionSiblings(list: TreeNode[]): SiblingUnit[] {
  const units: SiblingUnit[] = [];
  const keyOf = (node: TreeNode) => `${node.span.op} ${node.span.description}`;

  let index = 0;
  while (index < list.length) {
    const node = list[index] as TreeNode;
    const key = keyOf(node);

    let end = index;
    while (end + 1 < list.length) {
      const next = list[end + 1] as TreeNode;
      const breaksRun =
        next.children.length > 0 ||
        node.children.length > 0 ||
        keyOf(next) !== key;
      if (breaksRun) break;
      end += 1;
    }

    if (end - index + 1 >= AUTOGROUP_MIN) {
      units.push({ unit: 'run', nodes: list.slice(index, end + 1) });
      index = end + 1;
    } else {
      units.push({ unit: 'node', node });
      index += 1;
    }
  }

  return units;
}

/** The span a unit starts at, whichever kind of unit it is. */
function unitStart(entry: SiblingUnit): WaterfallSpan {
  return entry.unit === 'node'
    ? entry.node.span
    : (entry.nodes[0] as TreeNode).span;
}

function flatten(nodes: TreeNode[], state: FlattenState): Row[] {
  const rows: Row[] = [];

  /** Pushes one autogrouped run, open or folded. Returns the new frontier. */
  const emitRun = (
    entryNodes: TreeNode[],
    depth: number,
    guides: boolean[],
    last: boolean,
    frontier: number | null,
  ): number => {
    const spans = entryNodes.map((n) => n.span);
    const head = spans[0] as WaterfallSpan;
    const groupId = `group-${head.id}`;

    if (state.openGroups.has(groupId)) {
      for (const node of entryNodes) {
        rows.push({
          kind: 'span',
          id: node.span.id,
          span: node.span,
          depth,
          guides: [...guides, !last],
          hasChildren: false,
          collapsed: false,
          count: 0,
        });
      }
    } else {
      rows.push({
        kind: 'group',
        id: groupId,
        op: head.op ?? 'span',
        description: head.description,
        spans,
        depth,
        guides: [...guides, !last],
        failed: spans.some((s) => s.status && s.status !== 'ok'),
      });
    }

    return advance(frontier, ...spans.map(endOf));
  };

  /** Pushes one span and, when it is open, its children. Returns the frontier. */
  const emitNode = (
    node: TreeNode,
    depth: number,
    guides: boolean[],
    last: boolean,
    frontier: number | null,
  ): number => {
    const isCollapsed = state.collapsed.has(node.span.id);

    rows.push({
      kind: 'span',
      id: node.span.id,
      span: node.span,
      depth,
      guides: [...guides, !last],
      hasChildren: node.children.length > 0,
      collapsed: isCollapsed,
      count: descendants(node),
    });

    if (node.children.length > 0 && !isCollapsed) {
      walk(node.children, depth + 1, [...guides, !last]);
    }

    return advance(frontier, endOf(node.span));
  };

  const walk = (list: TreeNode[], depth: number, guides: boolean[]) => {
    const units = partitionSiblings(list);
    let previousEnd: number | null = null;

    units.forEach((entry, unitIndex) => {
      const last = unitIndex === units.length - 1;
      const first = unitStart(entry);

      if (
        state.showGaps &&
        previousEnd != null &&
        first.startMs - previousEnd >= GAP_MIN_MS
      ) {
        rows.push({
          kind: 'gap',
          id: `gap-${first.id}`,
          depth,
          guides: [...guides, true],
          startMs: previousEnd,
          endMs: first.startMs,
        });
      }

      previousEnd =
        entry.unit === 'run'
          ? emitRun(entry.nodes, depth, guides, last, previousEnd)
          : emitNode(entry.node, depth, guides, last, previousEnd);
    });
  };

  walk(nodes, 0, []);
  return rows;
}

/**
 * Flip one id in a set of ids: what a fold and an autogroup both do. Built
 * once, at module scope -- it closes over nothing but its setter.
 */
const toggleSet =
  (set: (updater: (previous: Set<string>) => Set<string>) => void) =>
  (id: string) => {
    set((previous) => {
      const next = new Set(previous);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

/* --- The component -------------------------------------------------------- */

/**
 * The hatched rows for unclaimed time between siblings, and what they are
 * called: "missing instrumentation". The name comes with the switch, so a
 * gap row can never render nameless.
 */
type MissingInstrumentationProps =
  | { showMissingInstrumentation?: false; missingInstrumentationLabel?: string }
  | { showMissingInstrumentation: true; missingInstrumentationLabel: string };

export type WaterfallProps = WaterfallBaseProps & MissingInstrumentationProps;

interface WaterfallBaseProps {
  spans: WaterfallSpan[];
  /**
   * The selected span, controlled: selection is a place the app may hold
   * in its own state or in the URL.
   */
  selectedId?: string | null;
  onSelect?: (span: WaterfallSpan | null) => void;
  /**
   * The selected span's detail, drawn inside the trace under its row --
   * attributes, links, the stack. The component owns where it opens; the
   * app owns what it says.
   */
  renderDetail?: (span: WaterfallSpan) => ReactNode;
  /** Names the figure: "Waterfall of POST /checkout/summary". */
  label: string;
  /** Stands for the duration of a span that never ended: "unfinished". */
  unfinishedLabel: string;
  /** The empty state's title and line: "No spans", "This trace carries no timing to draw." */
  noSpansLabel: string;
  noSpansHint: string;
  /** Names the column splitter: "Resize the name column". */
  resizeLabel: string;
  className?: string;
}

const MIN_SPLIT = 0.15;
const MAX_SPLIT = 0.85;

export function Waterfall(props: WaterfallProps) {
  const {
    spans,
    selectedId,
    onSelect,
    renderDetail,
    label,
    unfinishedLabel,
    noSpansLabel,
    noSpansHint,
    resizeLabel,
    className,
  } = props;
  // The gap rows' name, or null when they are off.
  const gapLabel = props.showMissingInstrumentation
    ? props.missingInstrumentationLabel
    : null;
  const rootRef = useRef<HTMLDivElement>(null);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [openGroups, setOpenGroups] = useState<Set<string>>(new Set());
  /*
   * The split as a fraction, not pixels: the same trace reads on a phone
   * and on an ultrawide, and the divider means the same thing on both.
   */
  const [split, setSplit] = useState(0.4);

  const tree = useMemo(() => buildTree(spans), [spans]);
  const rows = useMemo(
    () =>
      flatten(tree, {
        collapsed,
        openGroups,
        showGaps: gapLabel !== null,
      }),
    [tree, collapsed, openGroups, gapLabel],
  );

  const start = Math.min(...spans.map((span) => span.startMs));
  const end = Math.max(...spans.map(endOf));
  const total = Math.max(end - start, 1);

  const kinds = useMemo(() => {
    const present = new Set<SpanKind>();
    for (const span of spans) present.add(spanKind(span));
    return (
      [
        'agent',
        'llm',
        'tool',
        'handoff',
        'db',
        'internal',
        'http',
        'error',
      ] as const
    ).filter((kind) => present.has(kind));
  }, [spans]);

  const toggleCollapsed = toggleSet(setCollapsed);
  const toggleGroup = toggleSet(setOpenGroups);

  /*
   * No spans, no clock. `Math.min` of nothing is Infinity, so a ruler drawn
   * here would be a scale invented out of two infinities -- a trace that
   * measured nothing is a thing to say, not a thing to draw.
   */
  if (spans.length === 0) {
    return (
      <figure className={styles.root({ className })} aria-label={label}>
        <div className={styles.empty()}>
          <EmptyIcon size="2xl" aria-hidden="true" className="text-fg-ghost" />
          <div className="flex flex-col gap-1">
            <Text variant="card-title">{noSpansLabel}</Text>
            <Text variant="meta" tone="subtle">
              {noSpansHint}
            </Text>
          </div>
        </div>
      </figure>
    );
  }

  /* --- Divider ----------------------------------------------------------- */

  const onDividerPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    const root = rootRef.current;
    if (!root) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    const box = root.getBoundingClientRect();
    const move = (pointer: globalThis.PointerEvent) => {
      const fraction = (pointer.clientX - box.left) / box.width;
      setSplit(Math.min(Math.max(fraction, MIN_SPLIT), MAX_SPLIT));
    };
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  };

  const onDividerKeyDown = (event: KeyboardEvent) => {
    const step =
      event.key === 'ArrowLeft' ? -0.03 : event.key === 'ArrowRight' ? 0.03 : 0;
    if (step === 0) return;
    event.preventDefault();
    setSplit((previous) =>
      Math.min(Math.max(previous + step, MIN_SPLIT), MAX_SPLIT),
    );
  };

  /* --- Keyboard ---------------------------------------------------------- */

  /*
   * A roving walk: the arrows move between rows, Left/Right fold and
   * unfold, Enter and Space answer. One tab stop for the whole tree.
   */
  const moveFocus = (from: HTMLElement, delta: number | 'home' | 'end') => {
    const all = Array.from(
      rootRef.current?.querySelectorAll<HTMLElement>('[data-wf-row]') ?? [],
    );
    const index = all.indexOf(from);
    const target =
      delta === 'home'
        ? all[0]
        : delta === 'end'
          ? all[all.length - 1]
          : all[index + delta];
    target?.focus();
  };

  const gutterWidth = `${split * 100}%`;

  return (
    <figure
      ref={rootRef}
      style={{ '--wf-gutter': gutterWidth } as CSSProperties}
      className={styles.root({ className })}
      aria-label={label}
    >
      <div className={styles.body()}>
        <div className={styles.ruler()} aria-hidden="true">
          <div className="w-(--wf-gutter) shrink-0 @max-2xl:hidden" />
          <div className={styles.rulerTrack()}>
            {[0, 0.25, 0.5, 0.75, 1].map((fraction) => (
              <span key={fraction}>{formatSpanDuration(total * fraction)}</span>
            ))}
          </div>
        </div>

        <div role="tree" aria-label={label}>
          {rows.map((row, rowIndex) =>
            row.kind === 'gap' ? (
              // Gap rows only exist while `gapLabel` is set.
              gapLabel !== null && (
                <GapRow
                  key={row.id}
                  row={row}
                  start={start}
                  total={total}
                  label={gapLabel}
                />
              )
            ) : (
              <SpanRow
                key={row.id}
                row={row}
                start={start}
                total={total}
                tabStop={rowIndex === 0}
                selected={row.kind === 'span' && selectedId === row.id}
                onSelect={onSelect}
                renderDetail={renderDetail}
                onToggle={row.kind === 'group' ? toggleGroup : toggleCollapsed}
                moveFocus={moveFocus}
                unfinishedLabel={unfinishedLabel}
              />
            ),
          )}
        </div>

        {/* biome-ignore lint/a11y/useSemanticElements: a window splitter is
          ARIA-only -- <hr> cannot drag, focus or resize anything */}
        <div
          role="separator"
          aria-orientation="vertical"
          aria-label={resizeLabel}
          aria-valuenow={Math.round(split * 100)}
          aria-valuemin={MIN_SPLIT * 100}
          aria-valuemax={MAX_SPLIT * 100}
          tabIndex={0}
          style={{ left: `calc(16px + (100% - 32px) * ${split})` }}
          onPointerDown={onDividerPointerDown}
          onKeyDown={onDividerKeyDown}
          className={styles.divider()}
        />
      </div>

      <div className={styles.legend()}>
        {kinds.map((kind) => (
          <span key={kind} className={styles.legendItem()}>
            <span
              aria-hidden="true"
              className={styles.legendSwatch({ className: KIND_BAR[kind] })}
            />
            {kind}
          </span>
        ))}
      </div>
    </figure>
  );
}

Waterfall.displayName = 'Waterfall';

/* --- The rows ------------------------------------------------------------- */

/** One indent step per ancestor, carrying a guide line where one belongs. */
function RowGuides({ guides }: { guides: boolean[] }) {
  return (
    <>
      {guides.slice(0, -1).map((line, level) => (
        <span
          // Levels are positional by construction.
          key={level}
          aria-hidden="true"
          className={styles.guide()}
        >
          {line ? <span className={styles.guideLine()} /> : null}
        </span>
      ))}
    </>
  );
}

RowGuides.displayName = 'RowGuides';

/** The ruler's quarters, repeated down every track so bars stay readable. */
function RowGridlines() {
  return (
    <>
      {[25, 50, 75].map((position) => (
        <span
          key={position}
          aria-hidden="true"
          style={{ left: `${position}%` }}
          className={styles.gridline()}
        />
      ))}
    </>
  );
}

RowGridlines.displayName = 'RowGridlines';

/** Time inside the parent that no child claimed, hatched and named. */
function GapRow({
  row,
  start,
  total,
  label,
}: {
  row: Extract<Row, { kind: 'gap' }>;
  start: number;
  total: number;
  label: string;
}) {
  const left = ((row.startMs - start) / total) * 100;
  const width = ((row.endMs - row.startMs) / total) * 100;
  return (
    <div
      role="treeitem"
      tabIndex={-1}
      aria-level={row.depth + 1}
      aria-selected={false}
      aria-disabled="true"
      className={styles.row({ className: 'cursor-default' })}
    >
      <div className={styles.gutter()}>
        <RowGuides guides={row.guides} />
        <span className={styles.pillGap()} />
        <span className={styles.gapLabel()}>{label}</span>
      </div>
      <div className={styles.track()}>
        <RowGridlines />
        <span
          aria-hidden="true"
          style={{ left: `${left}%`, width: `${width}%` }}
          className={styles.gapBar()}
        />
        <DurationLabel left={left} width={width} ms={row.endMs - row.startMs} />
      </div>
    </div>
  );
}

GapRow.displayName = 'GapRow';

type SpanRowData = Extract<Row, { kind: 'span' } | { kind: 'group' }>;

/** What the row names: the op, what it did it to, and its error if it broke. */
function RowLabel({ row, failed }: { row: SpanRowData; failed: boolean }) {
  return (
    <>
      <span className={styles.op()}>
        {row.kind === 'group'
          ? `${row.op} ×${row.spans.length}`
          : (row.span.op ?? 'span')}
      </span>
      <span className={styles.description()}>
        {row.kind === 'group' ? row.description : row.span.description}
      </span>
      {failed ? (
        <Tag tone="error" className="shrink-0">
          {row.kind === 'span' ? (row.span.status ?? 'error') : 'error'}
        </Tag>
      ) : null}
    </>
  );
}

RowLabel.displayName = 'RowLabel';

/**
 * The bars: one for a span, one per member for an autogroup -- forty
 * queries as forty marks on one line is the reading that says N+1.
 */
function RowBars({
  row,
  kind,
  start,
  total,
  left,
  width,
}: {
  row: SpanRowData;
  kind: SpanKind;
  start: number;
  total: number;
  left: number;
  width: number;
}) {
  if (row.kind === 'group') {
    return (
      <>
        {row.spans.map((span) => (
          <span
            key={span.id}
            aria-hidden="true"
            style={{
              left: `${((span.startMs - start) / total) * 100}%`,
              width: `${Math.max(((endOf(span) - span.startMs) / total) * 100, 0.4)}%`,
            }}
            className={styles.bar({ className: KIND_BAR[kind] })}
          />
        ))}
      </>
    );
  }
  return (
    <span
      aria-hidden="true"
      style={{ left: `${left}%`, width: `${width}%` }}
      className={styles.bar({ className: KIND_BAR[kind] })}
    />
  );
}

RowBars.displayName = 'RowBars';

/** One span, or one autogrouped run of them, on the shared clock. */
/**
 * How a row reads: whether it failed, and which palette it draws in.
 *
 * A group takes the error look as soon as any span in it failed, because the
 * one that did is the reason someone is looking at the group at all.
 */
function rowAppearance(row: SpanRowData): { failed: boolean; kind: SpanKind } {
  if (row.kind === 'group') {
    return {
      failed: row.failed,
      kind: row.failed ? 'error' : spanKind(row.spans[0] as WaterfallSpan),
    };
  }

  const kind = spanKind(row.span);
  return { failed: kind === 'error', kind };
}

interface RowKeyActions {
  select: () => void;
  toggle: () => void;
  moveFocus: (from: HTMLElement, delta: number | 'home' | 'end') => void;
  expandable: boolean;
  isOpen: boolean;
}

/**
 * The treeitem keyboard contract, in one place.
 *
 * Arrow right opens and arrow left closes rather than toggling: that is what
 * the ARIA tree pattern specifies, and it is why these two are the only keys
 * here that do nothing when the row is already in the state they ask for.
 */
function rowKeyHandler({
  select,
  toggle,
  moveFocus,
  expandable,
  isOpen,
}: RowKeyActions) {
  return (event: KeyboardEvent<HTMLElement>) => {
    switch (event.key) {
      case 'Enter':
      case ' ':
        event.preventDefault();
        select();
        break;
      case 'ArrowDown':
        event.preventDefault();
        moveFocus(event.currentTarget, 1);
        break;
      case 'ArrowUp':
        event.preventDefault();
        moveFocus(event.currentTarget, -1);
        break;
      case 'Home':
        event.preventDefault();
        moveFocus(event.currentTarget, 'home');
        break;
      case 'End':
        event.preventDefault();
        moveFocus(event.currentTarget, 'end');
        break;
      case 'ArrowRight':
        if (expandable && !isOpen) toggle();
        break;
      case 'ArrowLeft':
        if (expandable && isOpen) toggle();
        break;
    }
  };
}

function SpanRow({
  row,
  start,
  total,
  tabStop,
  selected,
  onSelect,
  renderDetail,
  onToggle,
  moveFocus,
  unfinishedLabel,
}: {
  row: SpanRowData;
  start: number;
  total: number;
  tabStop: boolean;
  selected: boolean;
  onSelect?: (span: WaterfallSpan | null) => void;
  renderDetail?: (span: WaterfallSpan) => ReactNode;
  onToggle: (id: string) => void;
  moveFocus: (from: HTMLElement, delta: number | 'home' | 'end') => void;
  unfinishedLabel: string;
}) {
  const spans = row.kind === 'group' ? row.spans : [row.span];
  const rowStart = Math.min(...spans.map((s) => s.startMs));
  const rowEnd = Math.max(...spans.map(endOf));
  const { failed, kind } = rowAppearance(row);

  const left = ((rowStart - start) / total) * 100;
  // A 2 ms span in a 600 ms trace still has to be visible: it may be the
  // one that threw.
  const width = Math.min(
    Math.max(((rowEnd - rowStart) / total) * 100, 0.6),
    100 - left,
  );

  const expandable = row.kind === 'group' || row.hasChildren;
  const isOpen = row.kind === 'span' && row.hasChildren && !row.collapsed;
  const toggle = () => {
    if (expandable) onToggle(row.id);
  };
  const select = () => {
    if (row.kind !== 'span') {
      toggle();
      return;
    }
    onSelect?.(selected ? null : row.span);
  };

  const onRowKeyDown = rowKeyHandler({
    select,
    toggle,
    moveFocus,
    expandable,
    isOpen,
  });

  const onRowClick = (event: MouseEvent) => {
    const target = event.target as HTMLElement;
    if (
      target.closest('[data-chevron]') ||
      target.closest('[data-wf-detail]')
    ) {
      return;
    }
    select();
  };

  return (
    <div
      role="treeitem"
      // The tree is one tab stop; inside, the arrows take over.
      tabIndex={tabStop ? 0 : -1}
      data-wf-row
      aria-level={row.depth + 1}
      aria-selected={selected}
      aria-expanded={expandable ? isOpen : undefined}
      data-failed={failed || undefined}
      onClick={onRowClick}
      onKeyDown={onRowKeyDown}
      className={styles.row()}
    >
      <div className={styles.gutter()}>
        <RowGuides guides={row.guides} />
        {expandable ? (
          /* A pointer shortcut for the treeitem's own expand; the state is
             announced by aria-expanded above. */
          <span
            aria-hidden="true"
            data-chevron
            onClick={(event) => {
              event.stopPropagation();
              toggle();
            }}
            className={styles.pill()}
          >
            {isOpen ? (
              <ChevronDownIcon size="sm" aria-hidden="true" />
            ) : (
              <ChevronRightIcon size="sm" aria-hidden="true" />
            )}
            {row.kind === 'group' ? row.spans.length : row.count}
          </span>
        ) : (
          <span className={styles.pillGap()} />
        )}
        <RowLabel row={row} failed={failed} />
      </div>

      <div className={styles.track()}>
        <RowGridlines />
        <RowBars
          row={row}
          kind={kind}
          start={start}
          total={total}
          left={left}
          width={width}
        />
        <DurationLabel
          left={left}
          width={width}
          ms={spans.reduce((acc, s) => acc + (endOf(s) - s.startMs), 0)}
          // One member that never ended is the one worth looking at.
          unfinished={spans.some((s) => s.unfinished)}
          unfinishedLabel={unfinishedLabel}
          failed={failed}
        />
      </div>

      {selected && renderDetail && row.kind === 'span' ? (
        <div data-wf-detail className={styles.detail()}>
          {renderDetail(row.span)}
        </div>
      ) : null}
    </div>
  );
}

SpanRow.displayName = 'SpanRow';

/**
 * The duration beside its bar: outside its right end with room to spare,
 * inside the bar once the bar reaches far enough that "outside" would fall
 * off the edge.
 */
function DurationLabel({
  left,
  width,
  ms,
  unfinished,
  unfinishedLabel,
  failed,
}: {
  left: number;
  width: number;
  ms: number;
  unfinished?: boolean;
  /** Required whenever `unfinished` can be true. */
  unfinishedLabel?: string;
  failed?: boolean;
}) {
  const inside = left + width > 82;
  return (
    <span
      data-failed={failed || undefined}
      data-inside={inside || undefined}
      style={
        inside
          ? { right: `${Math.max(100 - left - width, 0)}%`, paddingRight: 6 }
          : { left: `${left + width}%`, paddingLeft: 6 }
      }
      className={styles.duration()}
    >
      {unfinished ? unfinishedLabel : formatSpanDuration(ms)}
    </span>
  );
}

DurationLabel.displayName = 'DurationLabel';
