// Not loaded through next/dynamic, deliberately.
//
// The advice does not apply in the App Router: this module is a client
// boundary, so recharts is already its own chunk and only ships to the three
// routes that draw charts. dynamic() would not remove it from those, only
// defer it past hydration — and on the overview and the agents page the
// charts are the content, so deferring them puts a hole where the number
// goes. It also cannot be written here: these are five named primitives used
// as JSX, and every consumer is a Server Component, where ssr: false throws.
// react-doctor-disable-next-line react-doctor/prefer-dynamic-import
import { Bar, BarChart, ResponsiveContainer, Tooltip, XAxis } from 'recharts';
import { useFormatter, useTranslations } from 'use-intl';
import {
  bucketWidth,
  type TimeAxis,
  timeBarAxis,
} from '@/shared/lib/time-axis';

interface EventChartProps {
  /** `[bucketStartUnixSeconds, count]` tuples from the issue stats endpoint. */
  data: [number, number][];
  height?: number;
}

function ChartTooltip(props: {
  active?: boolean;
  payload?: Array<{ payload: { t: number; count: number } }>;
  axis: TimeAxis;
}) {
  const format = useFormatter();
  const { active, payload, axis } = props;
  const t = useTranslations('charts');
  if (!active || !payload?.length) {
    return null;
  }
  const point = payload[0].payload;
  return (
    <div className="rounded-md border bg-surface-floating px-2.5 py-1.5 text-xs shadow-md">
      <p className="font-medium text-fg">
        {t('eventCount', { count: point.count })}
      </p>
      <p className="text-fg-subtle">
        {format.dateTime(new Date(point.t), axis.captionFormat, axis.zone)}
      </p>
    </div>
  );
}

/**
 * Event-count bar chart over time (Sentry-style trends). Built on recharts so
 * it shares the chart language used across the dashboard.
 */
export function EventChart({ data, height = 130 }: EventChartProps) {
  const format = useFormatter();
  const chartData = data.map(([t, count]) => ({ t: t * 1000, count }));
  // The issue stats are zero-filled, so the gap between points is the bucket.
  const times = chartData.map((p) => p.t);
  const axis = timeBarAxis(times, bucketWidth(times));

  return (
    <ResponsiveContainer width="100%" height={height}>
      <BarChart
        data={chartData}
        margin={{ top: 6, right: 4, bottom: 0, left: 0 }}
      >
        <XAxis
          dataKey="t"
          type="number"
          scale="time"
          domain={axis.domain}
          ticks={axis.ticks}
          interval="preserveStartEnd"
          tickFormatter={(v) =>
            format.dateTime(new Date(v), axis.format, axis.zone)
          }
          tick={{ fontSize: 11, fill: 'var(--muted-foreground)' }}
          axisLine={false}
          tickLine={false}
          minTickGap={48}
        />
        <Tooltip
          cursor={{ fill: 'var(--muted)', opacity: 0.5 }}
          content={<ChartTooltip axis={axis} />}
        />
        <Bar
          dataKey="count"
          fill="var(--primary)"
          radius={[2, 2, 0, 0]}
          maxBarSize={16}
          isAnimationActive={false}
        />
      </BarChart>
    </ResponsiveContainer>
  );
}
