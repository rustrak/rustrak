import { valueAt } from '../../lib/chart';
import { label, RUNS, type Run, SERIES } from '../../lib/compare';
import type { Series } from './line-chart.astro';

/*
 * The benchmark's two time series, shaped for `line-chart.astro`.
 *
 * Emphasis, not a palette: Rustrak in the brand lime, its PostgreSQL build a
 * softer lime, every other product in one neutral, named by a label at the
 * end of its line. Identity never rests on colour alone, and the table under
 * the charts carries every value.
 */

const data = SERIES as unknown as Record<
  string,
  { stored: [number, number][]; memory: [number, number][] }
>;

export const colorOf = (r: Run) =>
  r.id === 'rustrak'
    ? 'var(--fg-brand)'
    : r.rustrak
      ? 'color-mix(in oklab, var(--fg-brand) 55%, transparent)'
      : 'var(--fg-meta)';

/** A product's name, with its variant only where two runs share the name. */
const shortName = (run: Run) =>
  RUNS.filter((r) => r.name === run.name).length > 1
    ? `${run.name} · ${run.variant}`
    : run.name;

/**
 * Where each finish label sits, so close finishes do not collide: Rustrak to
 * the left of its dot, its PostgreSQL build to the right, GlitchTip above,
 * and the slow four stacked, each ending over its dot with a hairline down
 * to it.
 */
const PLACE: Record<
  string,
  { anchor: 'start' | 'middle' | 'end'; lift: number }
> = {
  rustrak: { anchor: 'end', lift: 8 },
  'rustrak-pg': { anchor: 'start', lift: 8 },
  glitchtip: { anchor: 'middle', lift: 24 },
  'sentry-errors': { anchor: 'end', lift: 10 },
  sentry: { anchor: 'end', lift: 26 },
  'bugsink-sqlite': { anchor: 'end', lift: 42 },
  bugsink: { anchor: 'end', lift: 58 },
};

/**
 * Events queryable in each product's database, second by second from the
 * first request, each line stopping where its product finished. Time is on
 * a log scale, which has no zero, so the lines start at 1 s; between two
 * counts the line is straight.
 */
export function race(): { series: Series[]; xMax: number } {
  const runs = RUNS.filter((r) => data[r.id]);
  const series = runs.map((run): Series => {
    const counts = data[run.id].stored;
    const total = counts[counts.length - 1][1];
    const done = counts[counts.findIndex(([, n]) => n === total)][0];
    const line = counts.slice(0, counts.findIndex(([t]) => t === done) + 1);
    const points: [number, number][] = [];
    for (let t = 1; t < done; t += 1) points.push([t, valueAt(line, t) ?? 0]);
    points.push([done, total]);
    const place = PLACE[run.id] ?? { anchor: 'end', lift: 8 };
    const leader = place.anchor === 'end' && place.lift > 8;
    return {
      id: run.id,
      label: label(run),
      color: colorOf(run),
      strong: !!run.rustrak,
      duration: run.rustrak ? 0.9 : 2.2,
      points,
      done,
      finish: `done in ${run.storedSecs} s`,
      end: {
        text: shortName(run),
        figure: `${run.storedSecs} s`,
        anchor: place.anchor,
        lift: place.lift,
        dx: { start: 6, middle: 0, end: leader ? 4 : -6 }[place.anchor],
        dy: 0,
        dot: true,
      },
    };
  });
  return { series, xMax: Math.max(...series.map((s) => s.done ?? 0)) };
}

/** Lines that end within a few pixels of each other: one label up, one down. */
const END_DY: Record<string, number> = { bugsink: -4, 'bugsink-sqlite': 12 };

/** Stack memory second by second around the burst, up to a minute in. */
export function memory(ids: string[]): Series[] {
  return RUNS.filter((r) => ids.includes(r.id) && data[r.id]).map((run) => ({
    id: run.id,
    label: label(run),
    color: colorOf(run),
    strong: !!run.rustrak,
    duration: 1.8,
    points: data[run.id].memory.filter(([t]) => t <= 60),
    end: {
      text: shortName(run),
      anchor: 'start',
      lift: 0,
      dx: 8,
      dy: END_DY[run.id] ?? 4,
      dot: false,
    },
  }));
}
