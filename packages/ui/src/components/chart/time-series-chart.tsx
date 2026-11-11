import { useId } from 'react';
// Not loaded through next/dynamic, deliberately: this package has no
// framework opinion, and recharts is already the only thing that pulls
// these primitives in -- deferring the import buys nothing a consumer's own
// code-splitting doesn't already give it.
// react-doctor-disable-next-line react-doctor/prefer-dynamic-import
import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { ChartLegend, ChartTooltip, XTick, YTick } from './chart-parts';
import { dataMax, integerTicks, isIsolated } from './chart-scale';
import { type ChartSeries, seriesColor } from './chart-series';

/**
 * A time series: events over time, adoption of a release, latency.
 *
 * Areas with a 2 px stroke and a gradient fill that falls to nothing -- the
 * line carries the reading, the fill only seats it on the baseline. Stacked,
 * the fills turn solid at low opacity instead: stacked gradients over
 * gradients turn to mud.
 *
 * The grid is horizontal only and dashed: enough to carry the eye to the
 * axis, not enough to cage the data. The crosshair and the tooltip are the
 * reading layer -- values live there and on the axis, never printed on
 * every point.
 */
export interface TimeSeriesChartProps<TDatum extends Record<string, unknown>> {
  data: TDatum[];
  series: ChartSeries[];
  /** The field that carries time. */
  xKey: string;
  /** Fixed height in pixels; the width follows the container. */
  height: number;
  /** Sum the series instead of overlaying them. */
  stacked?: boolean;
  formatX?: (value: string | number) => string;
  formatY?: (value: number) => string;
  /** Pin the Y scale: `[0, 100]` for a share. Absent, it follows the data. */
  yDomain?: [number, number];
  /** The values are counts: no `0.4` tick on an axis of whole things. */
  integer?: boolean;
  /** Names the figure for screen readers; the drawing stays explorable. */
  label: string;
  className?: string;
}

export function TimeSeriesChart<TDatum extends Record<string, unknown>>({
  data,
  series,
  xKey,
  height,
  stacked = false,
  formatX,
  formatY,
  yDomain,
  integer = false,
  label,
  className,
}: TimeSeriesChartProps<TDatum>) {
  // Gradient ids are document-global: with several charts on one page a
  // shared id would paint every chart with the first one's gradient.
  const id = useId();

  // Counts get their own evenly spaced whole ticks (see `integerTicks`).
  const ticks =
    integer && !yDomain
      ? integerTicks(
          dataMax(
            data,
            series.map((entry) => entry.key),
            stacked,
          ),
        )
      : undefined;

  return (
    <figure className={className} aria-label={label}>
      <ResponsiveContainer width="100%" height={height}>
        <AreaChart
          data={data}
          margin={{ top: 4, right: 20, bottom: 0, left: 0 }}
        >
          <defs>
            {series.map((entry, index) => (
              <linearGradient
                key={entry.key}
                id={`${id}-${entry.key}`}
                x1="0"
                y1="0"
                x2="0"
                y2="1"
              >
                <stop
                  offset="0%"
                  stopColor={seriesColor(entry, index)}
                  stopOpacity={0.25}
                />
                <stop
                  offset="100%"
                  stopColor={seriesColor(entry, index)}
                  stopOpacity={0}
                />
              </linearGradient>
            ))}
          </defs>

          <CartesianGrid
            vertical={false}
            stroke="var(--border-divider)"
            strokeDasharray="3 3"
          />
          <XAxis
            dataKey={xKey}
            axisLine={false}
            tickLine={false}
            tick={<XTick format={formatX} />}
            interval="preserveStartEnd"
            minTickGap={32}
          />
          <YAxis
            // Sized to its widest label: a fixed width cut "800 ms" to "00 ms".
            width="auto"
            axisLine={false}
            tickLine={false}
            tick={
              <YTick
                format={formatY ? (value) => formatY(Number(value)) : undefined}
              />
            }
            {...(ticks
              ? { ticks, interval: 0, domain: [0, Math.max(...ticks)] }
              : { tickCount: 4, allowDecimals: !integer, domain: yDomain })}
          />
          <Tooltip
            cursor={{ stroke: 'var(--border-strong)', strokeDasharray: '3 3' }}
            isAnimationActive={false}
            content={
              <ChartTooltip
                series={series}
                formatLabel={formatX}
                formatValue={formatY}
              />
            }
          />

          {series.map((entry, index) => (
            <Area
              key={entry.key}
              dataKey={entry.key}
              stackId={stacked ? 'stack' : undefined}
              type="monotone"
              stroke={seriesColor(entry, index)}
              strokeWidth={2}
              fill={
                stacked ? seriesColor(entry, index) : `url(#${id}-${entry.key})`
              }
              fillOpacity={stacked ? 0.28 : 1}
              // The dot only exists under the pointer: fifty resting dots
              // are texture, one active dot is an answer. A point with no
              // value on either side draws no line at all, so it gets its dot.
              dot={(dot: { index: number; cx?: number; cy?: number }) =>
                isIsolated(data, entry.key, dot.index) ? (
                  <circle
                    key={dot.index}
                    className="recharts-area-dot"
                    cx={dot.cx}
                    cy={dot.cy}
                    r={3.5}
                    fill={seriesColor(entry, index)}
                  />
                ) : (
                  <g key={dot.index} />
                )
              }
              activeDot={{
                r: 3.5,
                strokeWidth: 2,
                stroke: 'var(--surface)',
              }}
              /*
               * No entry animation, by doctrine: a series growing out of the
               * baseline explains nothing, and motion that explains nothing
               * is not drawn. What moves in a chart is the crosshair.
               */
              isAnimationActive={false}
            />
          ))}
        </AreaChart>
      </ResponsiveContainer>

      <ChartLegend series={series} />
    </figure>
  );
}

TimeSeriesChart.displayName = 'TimeSeriesChart';
