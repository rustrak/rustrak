import type {
  CleanupCounts,
  ProjectStorage,
  RetentionResponse,
  RetentionSettings,
  RetentionWindows,
  StorageAge,
} from '@rustrak/client';
import type { Translate } from '@/shared/lib/error-copy';
import { formatBytes } from '@/shared/lib/utils';

/** The kinds of data a retention window applies to. */
export type Kind = 'events' | 'transactions' | 'logs';

export const KINDS: readonly Kind[] = ['events', 'transactions', 'logs'];

/** The settings field that holds each kind's window. */
export const WINDOW_FIELD = {
  events: 'events_days',
  transactions: 'transactions_days',
  logs: 'logs_days',
} as const satisfies Record<Kind, keyof RetentionWindows>;

const BYTES_FIELD = {
  events: 'events_bytes',
  transactions: 'transactions_bytes',
  logs: 'logs_bytes',
} as const satisfies Record<Kind, keyof ProjectStorage>;

const COUNT_FIELD = {
  events: 'events_count',
  transactions: 'transactions_count',
  logs: 'logs_count',
} as const satisfies Record<Kind, keyof ProjectStorage>;

/**
 * The windows the page offers, in days; `null` keeps data forever. They are
 * the age buckets' bounds, so what a preset deletes is whole buckets.
 */
const PRESETS: readonly (number | null)[] = [7, 14, 30, 90, 180, 365, null];

/** The presets, plus a saved window outside them (set through the API). */
export function presetsWith(days: number | null): (number | null)[] {
  if (PRESETS.includes(days)) return [...PRESETS];
  const finite = [...PRESETS.filter((d) => d !== null), days as number].sort(
    (a, b) => a - b,
  );
  return [...finite, null];
}

/** The windows a project keeps its data for: its override, else the defaults. */
export function windowsFor(
  settings: RetentionSettings,
  projectId: number,
): RetentionWindows {
  const override = settings.overrides?.find((o) => o.project_id === projectId);
  if (!override) return settings.defaults;
  const { project_id: _, ...windows } = override;
  return windows;
}

/** Whether any project ever loses data, so the cleanup runs at all. */
export function deletesAnything(settings: RetentionSettings): boolean {
  return [settings.defaults, ...settings.overrides].some((w) =>
    KINDS.some((kind) => w[WINDOW_FIELD[kind]] != null),
  );
}

/**
 * The share of age bucket `index` past a `days` window, which the next
 * cleanup deletes: 1 for a bucket wholly older, a fraction for one the
 * window cuts (its rows taken as spread evenly over it). Bucket `i` spans
 * `bounds[i - 1]` to `bounds[i]` days; the last has no end, so a window
 * inside it counts nothing, a lower bound.
 */
export function bucketGoes(
  index: number,
  days: number | null,
  bounds: readonly number[],
): number {
  if (days === null) return 0;
  const start = index === 0 ? 0 : (bounds[index - 1] ?? 0);
  const end = bounds[index];
  if (start >= days) return 1;
  if (end === undefined || end <= days) return 0;
  return (end - days) / (end - start);
}

/** Bytes per kind, in one age bucket. */
export type KindBytes = Record<Kind, number>;

export interface AgeBucket extends KindBytes {
  /** Of those bytes, what the next cleanup deletes. */
  goes: KindBytes;
}

export interface ProjectShare {
  id: number;
  name: string;
  bytes: number;
  /** Of what every project holds. */
  share: number;
  byKind: KindBytes;
  custom: boolean;
}

export interface Summary {
  /** Everything, source maps included. */
  bytes: number;
  kinds: Record<Kind, { bytes: number; rows: number }>;
  sourceMaps: { files: number; bytes: number };
  /** One per age bucket, youngest first. */
  buckets: AgeBucket[];
  /** Bytes the next cleanup deletes, per the saved settings. */
  frees: number;
  /** Largest first. */
  projects: ProjectShare[];
}

export interface StorageData {
  projects: readonly ProjectStorage[];
  age: StorageAge;
  sourceMaps: { files: number; bytes: number };
  settings: RetentionSettings;
}

const zero = (): KindBytes => ({ events: 0, transactions: 0, logs: 0 });

/**
 * One project's rows priced into the age buckets (which it adds to), at its
 * own average row per kind, with what its own windows delete.
 */
function addToBuckets(
  p: ProjectStorage,
  data: StorageData,
  buckets: AgeBucket[],
): void {
  const age = data.age.projects.find((a) => a.project_id === p.project_id);
  const windows = windowsFor(data.settings, p.project_id);
  for (const kind of KINDS) {
    const held = p[COUNT_FIELD[kind]];
    const perRow = held > 0 ? p[BYTES_FIELD[kind]] / held : 0;
    age?.[kind].counts.forEach((count, i) => {
      const bucket = buckets[i];
      if (!bucket) return;
      const weight = Math.round(count * perRow);
      bucket[kind] += weight;
      bucket.goes[kind] += Math.round(
        weight *
          bucketGoes(i, windows[WINDOW_FIELD[kind]], data.age.bucket_days),
      );
    });
  }
}

/**
 * What the instance holds, each figure once: per kind, by age, and per
 * project. Bytes are the payload estimates.
 */
export function summarize(data: StorageData): Summary {
  const buckets: AgeBucket[] = Array.from(
    { length: data.age.bucket_days.length + 1 },
    () => ({ ...zero(), goes: zero() }),
  );
  const kinds = {
    events: { bytes: 0, rows: 0 },
    transactions: { bytes: 0, rows: 0 },
    logs: { bytes: 0, rows: 0 },
  };
  const projects: ProjectShare[] = data.projects.map((p) => {
    addToBuckets(p, data, buckets);
    const byKind = zero();
    for (const kind of KINDS) {
      byKind[kind] = p[BYTES_FIELD[kind]];
      kinds[kind].bytes += p[BYTES_FIELD[kind]];
      kinds[kind].rows += p[COUNT_FIELD[kind]];
    }
    return {
      id: p.project_id,
      name: p.project_name,
      bytes: KINDS.reduce((sum, k) => sum + byKind[k], 0),
      share: 0,
      byKind,
      custom: data.settings.overrides.some(
        (o) => o.project_id === p.project_id,
      ),
    };
  });
  const projectsTotal = projects.reduce((sum, p) => sum + p.bytes, 0);
  for (const p of projects) {
    p.share = projectsTotal > 0 ? p.bytes / projectsTotal : 0;
  }
  projects.sort((a, b) => b.bytes - a.bytes);

  return {
    bytes: projectsTotal + data.sourceMaps.bytes,
    kinds,
    sourceMaps: data.sourceMaps,
    buckets,
    frees: buckets.reduce(
      (sum, b) => sum + KINDS.reduce((s, k) => s + b.goes[k], 0),
      0,
    ),
    projects,
  };
}

/** Bytes the counted rows weigh, at the instance's average row per kind. */
export function estimateBytes(
  counts: CleanupCounts,
  projects: readonly ProjectStorage[],
): number {
  return Math.round(
    KINDS.reduce((sum, kind) => {
      const bytes = projects.reduce((t, p) => t + p[BYTES_FIELD[kind]], 0);
      const rows = projects.reduce((t, p) => t + p[COUNT_FIELD[kind]], 0);
      return sum + (rows > 0 ? (counts[kind] * bytes) / rows : 0);
    }, 0),
  );
}

/**
 * {@link estimateBytes} as words, plus `extra` bytes freed besides the rows.
 * `…` while the projects to price them are not in yet, rather than a 0 B that
 * reads as nothing worth mentioning.
 */
export function estimateSize(
  counts: CleanupCounts,
  projects: readonly ProjectStorage[] | null,
  extra = 0,
): string {
  if (projects === null) return '…';
  return formatBytes(estimateBytes(counts, projects) + extra);
}

/** "2 errors", "3 logs"… for what a run removed, leaving out the zeros. */
export function removedParts(removed: CleanupCounts, t: Translate): string[] {
  const parts: [number, string][] = [
    [removed.events, 'unit.errors'],
    [removed.transactions, 'unit.transactions'],
    [removed.spans, 'unit.spans'],
    [removed.logs, 'unit.logs'],
    [removed.issues_removed, 'unit.emptyIssues'],
    [removed.source_maps, 'unit.sourceMaps'],
  ];
  return parts
    .filter(([count]) => count > 0)
    .map(([count, key]) => t(key, { count }));
}

/** How often the status is read while it is about to change. */
const POLL_MS = 15_000;
/** `setTimeout` overflows past this and fires at once. */
const MAX_TIMEOUT_MS = 2_147_483_647;

/**
 * When to read the saved status again, in ms, or `null` for never. It
 * changes on its own: a running cleanup finishes, and a scheduled one runs at
 * its time.
 */
export function nextReadIn(
  state: RetentionResponse,
  now: number,
): number | null {
  if (state.last_run?.status === 'running') return POLL_MS;
  if (!deletesAnything(state.settings) || !state.next_run_at) return null;
  const due = Date.parse(state.next_run_at) - now;
  return Math.min(Math.max(due, POLL_MS), MAX_TIMEOUT_MS);
}

/**
 * A count's read as the value {@link impactState} takes: the counts once in,
 * `'failed'` once a read failed and no retry is running, `null` meanwhile.
 */
export function impactOf<T>(read: {
  data: T | undefined;
  isError: boolean;
  isFetching: boolean;
}): T | null | 'failed' {
  if (read.data !== undefined) return read.data;
  return read.isError && !read.isFetching ? 'failed' : null;
}

/** How far a count of what goes has got: pending, failed, or in. */
export function impactState(
  impact: object | null | 'failed',
): 'counting' | 'failed' | 'ready' {
  if (impact === 'failed') return 'failed';
  return impact === null ? 'counting' : 'ready';
}

/**
 * How long the job has run, as `m:ss`: what a compaction, which reports no
 * progress of its own, shows while it runs.
 */
export function elapsed(since: string | null, now: number): string {
  const seconds = since
    ? Math.max(0, Math.floor((now - Date.parse(since)) / 1000))
    : 0;
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}
