// Imported statically where the chart is the content (the overview), and
// through `lazy()` where it is not (the event page).
// react-doctor-disable-next-line react-doctor/prefer-dynamic-import
import { Bar, BarChart, ResponsiveContainer, Tooltip, XAxis } from 'recharts';
import { useFormatter, useTranslations } from 'use-intl';

interface EventChartProps {
  /** `[bucketStartUnixSeconds, count]` tuples from the issue stats endpoint. */
  data: [number, number][];
  height?: number;
}

function ChartTooltip(props: {
  active?: boolean;
  payload?: Array<{ payload: { t: number; count: number } }>;
}) {
  const format = useFormatter();
  const { active, payload } = props;
  const t = useTranslations('charts');
  if (!active || !payload?.length) {
    return null;
  }
  const point = payload[0].payload;
  return (
    <div className="rounded-md border bg-popover px-2.5 py-1.5 text-xs shadow-md">
      <p className="font-medium text-popover-foreground">
        {t('eventCount', { count: point.count })}
      </p>
      <p className="text-muted-foreground">
        {format.dateTime(new Date(point.t), 'date')}
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
          domain={['dataMin', 'dataMax']}
          tickFormatter={(v) => format.dateTime(new Date(v), 'axisDay')}
          tick={{ fontSize: 11, fill: 'var(--muted-foreground)' }}
          axisLine={false}
          tickLine={false}
          minTickGap={48}
        />
        <Tooltip
          cursor={{ fill: 'var(--muted)', opacity: 0.5 }}
          content={<ChartTooltip />}
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
