// Not loaded through next/dynamic, deliberately: this package has no
// framework opinion, and recharts is already the only thing that pulls
// these primitives in -- deferring the import buys nothing a consumer's own
// code-splitting doesn't already give it.
// react-doctor-disable-next-line react-doctor/prefer-dynamic-import
import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { ChartLegend, ChartTooltip, XTick, YTick } from './chart-parts';
import { dataMax, integerTicks } from './chart-scale';
import { type ChartSeries, seriesColor } from './chart-series';

/**
 * Bucketed bars over time: the events-by-severity chart, a log volume.
 *
 * Stacked by default, because that is what the product asks of it -- one
 * column per bucket, severity as bands. The bands separate with a 1 px
 * stroke of the surface, not a lighter shade: a real gap survives every
 * kind of colour-blindness, which is what lets error sit next to warning.
 * Only the top of the column is rounded; data grows from the baseline and
 * the baseline stays sharp.
 *
 * When the series are severities, pass their colours (`var(--sev-error)`…):
 * status colours are reserved and never come from the categorical order.
 */
export interface BarsChartProps<TDatum extends Record<string, unknown>> {
  data: TDatum[];
  series: ChartSeries[];
  xKey: string;
  height: number;
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

export function BarsChart<TDatum extends Record<string, unknown>>({
  data,
  series,
  xKey,
  height,
  stacked = true,
  formatX,
  formatY,
  yDomain,
  integer = false,
  label,
  className,
}: BarsChartProps<TDatum>) {
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
        <BarChart
          data={data}
          margin={{ top: 4, right: 20, bottom: 0, left: 0 }}
          barCategoryGap="24%"
        >
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
            cursor={{ fill: 'var(--surface-hover)' }}
            isAnimationActive={false}
            content={
              <ChartTooltip
                series={series}
                formatLabel={formatX}
                formatValue={formatY}
              />
            }
          />

          {series.map((entry, index) => {
            const top = index === series.length - 1;
            return (
              <Bar
                key={entry.key}
                dataKey={entry.key}
                stackId={stacked ? 'stack' : undefined}
                fill={seriesColor(entry, index)}
                // The 1 px surface seam between stacked bands.
                stroke="var(--surface)"
                strokeWidth={1}
                radius={top || !stacked ? [2, 2, 0, 0] : 0}
                // See TimeSeriesChart: entry animation explains nothing.
                isAnimationActive={false}
              />
            );
          })}
        </BarChart>
      </ResponsiveContainer>

      <ChartLegend series={series} />
    </figure>
  );
}

BarsChart.displayName = 'BarsChart';
