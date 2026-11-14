'use client';

import { useInView } from 'motion/react';
import { type ReactNode, useRef } from 'react';
import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  LabelList,
  Line,
  LineChart,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { label, mb, num, RUNS, type Run } from '@/lib/compare';
import SERIES from '@/lib/compare-series.json';

/*
 * Emphasis, not a palette: Rustrak in the brand lime, every other product in
 * one neutral, named by a label at the end of its line. Identity never rests on
 * color alone, and the table under the charts carries every value.
 */
const BRAND = 'var(--fg-brand)';
const BRAND_SOFT = 'color-mix(in oklab, var(--fg-brand) 55%, transparent)';
const OTHER = 'var(--fg-meta)';
const GRID = 'var(--border-divider)';
const TICK = {
  fill: 'var(--fg-meta)',
  fontSize: 11,
  fontFamily: 'var(--font-mono, monospace)',
};

const data = SERIES as Record<
  string,
  { stored: number[][]; memory: number[][] }
>;

const colorOf = (r: Run) =>
  r.id === 'rustrak' ? BRAND : r.rustrak ? BRAND_SOFT : OTHER;

/** Draws its chart the first time it scrolls into view, so the lines animate then. */
function OnView({ height, children }: { height: number; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const seen = useInView(ref, { once: true, margin: '-15% 0px' });
  return (
    <div ref={ref} style={{ height }}>
      {seen ? children : null}
    </div>
  );
}

/** Rows keyed by second, one column per run; only measured points are set. */
function merge(runs: Run[], pick: (id: string) => number[][]) {
  const rows = new Map<number, Record<string, number>>();
  for (const r of runs) {
    for (const [t, v] of pick(r.id)) {
      const row = rows.get(t) ?? { t };
      row[r.id] = v;
      rows.set(t, row);
    }
  }
  return [...rows.values()].sort((a, b) => a.t - b.t);
}

function Panel({ children }: { children: ReactNode }) {
  return (
    <div className="rounded-xl border border-border-subtle bg-canvas/95 px-3 py-2 shadow-lg backdrop-blur">
      {children}
    </div>
  );
}

function TipRows({
  title,
  rows,
}: {
  title: string;
  rows: { run: Run; value: string }[];
}) {
  return (
    <Panel>
      <p className="mb-1.5 font-mono text-fg-meta text-mono">{title}</p>
      <ul className="flex flex-col gap-1">
        {rows.map(({ run, value }) => (
          <li key={run.id} className="flex items-center gap-2 text-meta">
            <span
              className="size-2 rounded-full"
              style={{ background: colorOf(run) }}
            />
            <span className={run.rustrak ? 'text-fg' : 'text-fg-subtle'}>
              {label(run)}
            </span>
            <span className="ms-auto ps-4 font-mono text-fg tabular-nums">
              {value}
            </span>
          </li>
        ))}
      </ul>
    </Panel>
  );
}

function Legend({ runs }: { runs: Run[] }) {
  return (
    <ul className="flex flex-wrap gap-x-5 gap-y-2 text-fg-subtle text-meta">
      {runs.map((r) => (
        <li key={r.id} className="flex items-center gap-2">
          <span
            className="h-0.5 w-4 rounded-full"
            style={{ background: colorOf(r) }}
          />
          {label(r)}
        </li>
      ))}
    </ul>
  );
}

const shortName = (run: Run) =>
  RUNS.filter((r) => r.name === run.name).length > 1
    ? `${run.name} · ${run.variant}`
    : run.name;

/** A name at the last measured point of a line. */
function endLabel(run: Run, last: number) {
  return (props: {
    x?: number | string;
    y?: number | string;
    index?: number;
  }) => {
    if (props.index !== last) return null;
    return (
      <text
        x={Number(props.x) + 8}
        y={Number(props.y)}
        dy={4}
        fill={run.rustrak ? 'var(--fg)' : 'var(--fg-subtle)'}
        fontSize={11}
      >
        {shortName(run)}
      </text>
    );
  };
}

/** Events queryable in each product's database, seconds from the first request. */
export function RaceChart({ ids }: { ids?: string[] }) {
  const runs = RUNS.filter((r) => data[r.id] && (!ids || ids.includes(r.id)));
  // Each line stops where its product finished: the race ends at the line.
  const lines = Object.fromEntries(
    runs.map((r) => {
      const points = data[r.id].stored;
      const done = points.findIndex(([, n]) => n === points.at(-1)?.[1]);
      return [r.id, points.slice(0, done + 1)];
    }),
  );
  const doneAt = (id: string) => lines[id].at(-1)?.[0] ?? 0;
  const xMax = Math.max(...runs.map((r) => doneAt(r.id)));

  /** Measured where a count exists, straight between counts, nothing after the finish. */
  const valueAt = (id: string, t: number) => {
    const points = lines[id];
    if (t > doneAt(id)) return null;
    const i = points.findIndex(([pt]) => pt >= t);
    if (points[i][0] === t) return points[i][1];
    const [t0, n0] = points[i - 1];
    const [t1, n1] = points[i];
    return Math.round(n0 + ((n1 - n0) * (t - t0)) / (t1 - t0));
  };

  // One row per second plus every finish, so the tooltip has every product at
  // every position. Time is on a log scale, which has no zero: it starts at 1 s.
  const times = new Set<number>();
  for (let t = 1; t <= xMax; t += 1) times.add(t);
  for (const r of runs) times.add(doneAt(r.id));
  const rows = [...times]
    .sort((a, b) => a - b)
    .map((t) => {
      const row: Record<string, number> = { t };
      for (const r of runs) {
        const v = valueAt(r.id, t);
        if (v != null) row[r.id] = v;
      }
      return row;
    });

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <Legend runs={runs} />
      </div>
      <OnView height={380}>
        <ResponsiveContainer>
          <LineChart
            data={rows}
            margin={{ top: 72, right: 24, bottom: 8, left: 0 }}
          >
            <CartesianGrid stroke={GRID} vertical={false} />
            <XAxis
              dataKey="t"
              type="number"
              scale="log"
              domain={[1, xMax]}
              ticks={[1, 2, 5, 10, 20, 50, 100, 200].filter((t) => t <= xMax)}
              tick={TICK}
              tickLine={false}
              axisLine={{ stroke: GRID }}
              tickFormatter={(v) => `${v} s`}
            />
            <YAxis
              domain={[0, 20_000]}
              ticks={[0, 5_000, 10_000, 15_000, 20_000]}
              tick={TICK}
              tickLine={false}
              axisLine={false}
              width={52}
              tickFormatter={(v) => (v ? `${v / 1000}k` : '0')}
            />
            <Tooltip
              cursor={{ stroke: 'var(--fg-ghost)', strokeWidth: 1 }}
              position={{ y: 0 }}
              isAnimationActive={false}
              content={({ active, label }) => {
                if (!active || label == null) return null;
                const t = Number(label);
                const status = runs
                  .map((run) => ({
                    run,
                    done: doneAt(run.id) <= t,
                    n: valueAt(run.id, t) ?? 0,
                  }))
                  .sort((a, b) =>
                    a.done && b.done
                      ? doneAt(a.run.id) - doneAt(b.run.id)
                      : Number(b.done) - Number(a.done) || b.n - a.n,
                  );
                return (
                  <TipRows
                    title={`${t} s after the first request`}
                    rows={status.map(({ run, done, n }) => ({
                      run,
                      value: done
                        ? `done in ${run.storedSecs} s`
                        : `${num(n)} stored`,
                    }))}
                  />
                );
              }}
            />
            {runs.map((r) => {
              const end = rows.findLastIndex((row) => row[r.id] != null);
              return (
                <Line
                  key={r.id}
                  dataKey={r.id}
                  type="linear"
                  connectNulls
                  stroke={colorOf(r)}
                  strokeWidth={r.rustrak ? 2.5 : 1.75}
                  dot={false}
                  activeDot={{
                    r: 4,
                    strokeWidth: 2,
                    stroke: 'var(--surface-canvas)',
                  }}
                  animationDuration={r.rustrak ? 900 : 2200}
                  animationEasing="ease-out"
                >
                  <LabelList content={finishLabel(r, end)} />
                </Line>
              );
            })}
          </LineChart>
        </ResponsiveContainer>
      </OnView>
    </div>
  );
}

/** A dot and the time where a product's line reaches its last event. */
/**
 * Where each finish label sits, so close finishes do not collide: Rustrak to
 * the left of its dot, its PostgreSQL build to the right, the rest above.
 */
const PLACE: Record<
  string,
  { anchor: 'start' | 'middle' | 'end'; lift: number }
> = {
  rustrak: { anchor: 'end', lift: 8 },
  'rustrak-pg': { anchor: 'start', lift: 8 },
  glitchtip: { anchor: 'middle', lift: 24 },
  // The slow four finish close together: stacked, each ending over its dot
  // with a hairline down to it.
  'sentry-errors': { anchor: 'end', lift: 10 },
  sentry: { anchor: 'end', lift: 26 },
  'bugsink-sqlite': { anchor: 'end', lift: 42 },
  bugsink: { anchor: 'end', lift: 58 },
};

function finishLabel(run: Run, end: number) {
  const place = PLACE[run.id] ?? { anchor: 'end' as const, lift: 8 };
  // Stacked labels end just past their dot, with a hairline down to it.
  const leader = place.anchor === 'end' && place.lift > 8;
  const DX = { start: 6, middle: 0, end: leader ? 4 : -6 };
  const ink = run.rustrak ? 'var(--fg)' : 'var(--fg-subtle)';
  const figure = run.rustrak ? 'var(--fg-brand)' : 'var(--fg-secondary)';
  return (props: {
    x?: number | string;
    y?: number | string;
    index?: number;
  }) => {
    const x = Number(props.x);
    const y = Number(props.y);
    if (props.index !== end) return null;
    return (
      <g>
        {leader ? (
          <line
            x1={x}
            x2={x}
            y1={y - place.lift + 4}
            y2={y - 4}
            stroke="var(--fg-ghost)"
            strokeWidth={1}
          />
        ) : null}
        <circle
          cx={x}
          cy={y}
          r={3.5}
          fill={colorOf(run)}
          stroke="var(--surface-panel, #1c1c1c)"
          strokeWidth={2}
        />
        <text
          x={x}
          y={y - place.lift}
          dx={DX[place.anchor]}
          textAnchor={place.anchor}
          fill={ink}
          fontSize={11}
        >
          {shortName(run)}{' '}
          <tspan fill={figure} fontFamily="var(--font-mono, monospace)">
            {run.storedSecs} s
          </tspan>
        </text>
      </g>
    );
  };
}

/** Stack memory second by second around the burst, for the products that fit one scale. */
export function MemoryChart({ ids }: { ids: string[] }) {
  const runs = RUNS.filter((r) => ids.includes(r.id) && data[r.id]);
  const rows = merge(runs, (id) => data[id].memory).filter((r) => r.t <= 60);

  return (
    <div className="flex flex-col gap-5">
      <Legend runs={runs} />
      <OnView height={320}>
        <ResponsiveContainer>
          <LineChart
            data={rows}
            margin={{ top: 8, right: 128, bottom: 8, left: 0 }}
          >
            <CartesianGrid stroke={GRID} vertical={false} />
            <XAxis
              dataKey="t"
              type="number"
              domain={[-20, 60]}
              ticks={[-20, 0, 20, 40, 60]}
              tick={TICK}
              tickLine={false}
              axisLine={{ stroke: GRID }}
              tickFormatter={(v) => `${v} s`}
            />
            <YAxis
              tick={TICK}
              tickLine={false}
              axisLine={false}
              width={52}
              tickFormatter={(v) => mb(Number(v))}
            />
            <ReferenceLine
              x={0}
              stroke="var(--fg-ghost)"
              label={{
                value: 'burst starts',
                position: 'insideTopLeft',
                fill: 'var(--fg-meta)',
                fontSize: 11,
              }}
            />
            <Tooltip
              cursor={{ stroke: 'var(--fg-ghost)', strokeWidth: 1 }}
              content={({ active, label: t, payload }) =>
                active && payload?.length ? (
                  <TipRows
                    title={`${t} s`}
                    rows={payload.flatMap((p) => {
                      const run = runs.find((r) => r.id === p.dataKey);
                      return run && p.value != null
                        ? [{ run, value: mb(Number(p.value)) }]
                        : [];
                    })}
                  />
                ) : null
              }
            />
            {runs.map((r) => {
              const points = rows.flatMap((row, i) =>
                row[r.id] != null ? [i] : [],
              );
              return (
                <Line
                  key={r.id}
                  dataKey={r.id}
                  type="monotone"
                  connectNulls
                  stroke={colorOf(r)}
                  strokeWidth={r.rustrak ? 2.5 : 1.75}
                  dot={false}
                  activeDot={{
                    r: 4,
                    strokeWidth: 2,
                    stroke: 'var(--surface-canvas)',
                  }}
                  animationDuration={1800}
                >
                  <LabelList content={endLabel(r, points.at(-1) ?? -1)} />
                </Line>
              );
            })}
          </LineChart>
        </ResponsiveContainer>
      </OnView>
    </div>
  );
}

/** One measure per run as horizontal bars, Rustrak lit, the value at each bar's end. */
export function RankBars({
  metric,
  unit,
  ids,
}: {
  metric: 'peak' | 'cpu' | 'stored';
  unit: string;
  ids?: string[];
}) {
  const value = (r: Run) =>
    metric === 'peak'
      ? r.ram.peak
      : metric === 'cpu'
        ? r.cpuMsPerEvent
        : r.storedPerS;
  const format = (v: number) =>
    metric === 'peak'
      ? mb(v)
      : metric === 'cpu'
        ? `${v} ${unit}`
        : `${num(v)} ${unit}`;
  const rows = RUNS.filter((r) => !ids || ids.includes(r.id))
    .map((r) => ({
      name: label(r),
      value: value(r),
      run: r,
    }))
    .sort((a, b) =>
      metric === 'stored' ? b.value - a.value : a.value - b.value,
    );

  return (
    <OnView height={rows.length * 44 + 16}>
      <ResponsiveContainer>
        <BarChart
          data={rows}
          layout="vertical"
          margin={{ top: 0, right: 88, bottom: 0, left: 0 }}
          barCategoryGap={12}
        >
          <XAxis type="number" hide domain={[0, 'dataMax']} />
          <YAxis
            type="category"
            dataKey="name"
            width={190}
            tick={{ fill: 'var(--fg-subtle)', fontSize: 12 }}
            tickLine={false}
            axisLine={false}
          />
          <Tooltip
            cursor={{ fill: 'var(--surface)', opacity: 0.5 }}
            content={({ active, payload }) => {
              const row = active ? payload?.[0]?.payload : null;
              return row ? (
                <TipRows
                  title={row.name}
                  rows={[{ run: row.run, value: format(row.value) }]}
                />
              ) : null;
            }}
          />
          <Bar
            dataKey="value"
            radius={[0, 4, 4, 0]}
            minPointSize={3}
            animationDuration={1400}
          >
            {rows.map((row) => (
              <Cell
                key={row.run.id}
                fill={row.run.rustrak ? colorOf(row.run) : OTHER}
              />
            ))}
            <LabelList
              dataKey="value"
              position="right"
              formatter={(v: unknown) => format(Number(v))}
              style={{
                fill: 'var(--fg-secondary)',
                fontSize: 11,
                fontFamily: 'var(--font-mono, monospace)',
              }}
            />
          </Bar>
        </BarChart>
      </ResponsiveContainer>
    </OnView>
  );
}
