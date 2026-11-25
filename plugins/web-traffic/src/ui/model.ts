// What the plugin's routes answer, as the UI reads it.

/** The windows on offer: the dashboard's, plus a quarter. */
export const PERIODS = ['24h', '7d', '14d', '30d', '90d'] as const;
export type Period = (typeof PERIODS)[number];
/** The window a page opens on, as every project page does. */
export const DEFAULT_PERIOD: Period = '14d';

/** The page's address query, as `usePluginSearch()` reads it. */
type Search = Partial<Record<string, string>>;

/** The window the address holds, or the default one. */
export function periodOf(search: Search): Period {
  return (PERIODS as readonly string[]).includes(search.period ?? '')
    ? (search.period as Period)
    : DEFAULT_PERIOD;
}

/** What the report can be narrowed by: the address keys the routes read. */
export const DIMENSIONS = [
  // Not `page`: the dashboard keeps that for which page of a list is open,
  // and a saved view leaves it out.
  'path',
  'title',
  'referrer',
  'source',
  'medium',
  'campaign',
  'country',
  'language',
  'browser',
  'os',
  'device',
] as const;
export type Dimension = (typeof DIMENSIONS)[number];

/** What a top list ranks: a dimension, a visit's first or last page, events. */
export const RANKINGS = [...DIMENSIONS, 'entry', 'exit', 'event'] as const;
export type Ranking = (typeof RANKINGS)[number];

/** The most a list route answers (`MAX_ROWS` in models/stats.rs). */
export const MAX_ROWS = 500;

/** Where an event's own page is. */
export const eventPage = (name: string) => `events/${encodeURIComponent(name)}`;

/** How a chart's ticks read: the hour over a day, the day otherwise. */
export function tickFormat(period: Period) {
  return period === '24h'
    ? ({ hour: 'numeric' } as const)
    : ({ day: 'numeric', month: 'short' } as const);
}

/** The dimension a ranked value narrows by: an entry page is a page. */
export function narrows(by: Ranking): Dimension | null {
  if (by === 'entry' || by === 'exit') return 'path';
  return by === 'event' ? null : by;
}

/**
 * The narrowing the page's address holds: the window, always valid, and
 * each dimension set. What a route is asked with.
 */
export function filterOf(search: Search): Record<string, string> {
  const out: Record<string, string> = { period: periodOf(search) };
  for (const d of DIMENSIONS) {
    const value = search[d];
    if (value) out[d] = value;
  }
  return out;
}

export interface Totals {
  pageviews: number;
  visitors: number;
  bounces: number;
  /** Seconds between each visitor's first and last page, summed. */
  duration: number;
}

export interface Overview {
  current: Totals;
  previous: Totals;
  series: { t: number; pageviews: number; visitors: number }[];
}

export interface Row {
  value: string;
  visitors: number;
  count: number;
  /** Of `visitors`, those who saw one page all day. */
  bounces?: number;
  /** Their visits' length in seconds, summed. */
  duration?: number;
}

/** Share of visitors who saw one page, 0 to 100; `null` without visitors. */
export function bounceRate(totals: Totals): number | null {
  return share(totals.bounces, totals.visitors);
}

/** The change against the period before, in percent; `null` from nothing. */
export function change(now: number, before: number): number | null {
  return before === 0 ? null : Math.round(((now - before) / before) * 100);
}

/** The path of a page's URL, or `null` when it is not one. */
export function pathOf(url: unknown): string | null {
  if (typeof url !== 'string') return null;
  try {
    return new URL(url).pathname;
  } catch {
    return null;
  }
}

/** The public key of a DSN: `https://<key>@host/<project>`. */
export function keyOf(dsn: string | null | undefined): string | null {
  if (!dsn) return null;
  try {
    return new URL(dsn).username || null;
  } catch {
    return null;
  }
}

/** The average visit, in seconds; `null` without visitors. */
export function visitLength(
  totals: Pick<Totals, 'duration' | 'visitors'>,
): number | null {
  return totals.visitors === 0
    ? null
    : Math.round(totals.duration / totals.visitors);
}

/** `1h 4m`, `3m 20s`, `12s`. */
export function duration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

export interface Visitor {
  id: string;
  first: number;
  last: number;
  pageviews: number;
  events: number;
  entry: string;
  referrer: string | null;
  country: string | null;
  browser: string | null;
  os: string | null;
  device: string | null;
}

export interface Step {
  kind: 'pageview' | 'event';
  at: number;
  path: string;
  label: string | null;
  data: string | null;
}

export interface Property {
  key: string;
  value: string;
  count: number;
}

/** An event's properties, each with its values. */
export function byKey(properties: readonly Property[]): [string, Property[]][] {
  const groups = new Map<string, Property[]>();
  for (const p of properties)
    groups.set(p.key, [...(groups.get(p.key) ?? []), p]);
  return [...groups];
}

export interface Profile {
  id: string;
  first: number;
  last: number;
  pageviews: number;
  events: number;
  pages: number;
  entry: string;
  exit: string;
  referrer: string | null;
  source: string | null;
  campaign: string | null;
  country: string | null;
  language: string | null;
  browser: string | null;
  os: string | null;
  device: string | null;
}

/** Events by name, one row per bucket of the window: `{ t, signup: 3 }`. */
export interface EventSeries {
  names: string[];
  /** `t` in unix seconds. */
  rows: ({ t: number } & Record<string, number>)[];
}

export interface EventTotals {
  count: number;
  visitors: number;
}

export interface Occurrence {
  at: number;
  path: string;
  visitor: string;
  data: string | null;
}

/** A pause this long between two steps starts a new visit, as Umami does. */
const VISIT_GAP = 30 * 60;

/** A day's steps split into visits. */
export function visits(steps: readonly Step[]): Step[][] {
  const out: Step[][] = [];
  for (const step of steps) {
    const current = out.at(-1);
    const last = current?.at(-1);
    if (!current || !last || step.at - last.at > VISIT_GAP) out.push([step]);
    else current.push(step);
  }
  return out;
}

/** A share in percent, rounded; `null` from nothing. */
export function share(part: number, whole: number): number | null {
  return whole === 0 ? null : Math.round((part / whole) * 100);
}
