import type { AgentDurationPoint } from '@rustrak/client';
// Static: where this chart renders, it is the content of the page.
// react-doctor-disable-next-line react-doctor/prefer-dynamic-import
import { Line, LineChart, ResponsiveContainer, Tooltip, XAxis } from 'recharts';
import { useFormatter, useTranslations } from 'use-intl';

interface AgentDurationChartProps {
  points: AgentDurationPoint[];
  height?: number;
}

function formatMs(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

function ChartTooltip({
  active,
  payload,
}: {
  active?: boolean;
  payload?: Array<{ payload: { t: number; avg: number; p95: number } }>;
}) {
  const format = useFormatter();
  const t = useTranslations('agents');
  if (!active || !payload?.length) {
    return null;
  }
  const point = payload[0].payload;
  return (
    <div className="rounded-md border bg-popover px-2.5 py-1.5 text-xs shadow-md">
      <p className="font-medium text-popover-foreground">
        {t('charts.durationTooltip', {
          avg: formatMs(point.avg),
          p95: formatMs(point.p95),
        })}
      </p>
      <p className="text-muted-foreground">
        {format.dateTime(new Date(point.t), 'dateTime')}
      </p>
    </div>
  );
}

/**
 * Duration avg/p95 line chart. Same recharts styling convention as
 * `AgentTimeseriesChart`/`EventChart` — hidden axis lines, CSS-var colors,
 * no animation.
 */
export function AgentDurationChart({
  points,
  height = 130,
}: AgentDurationChartProps) {
  const format = useFormatter();
  const t = useTranslations('agents');
  const chartData = points.map((p) => ({
    t: new Date(p.bucket).getTime(),
    avg: p.avg_ms,
    p95: p.p95_ms,
  }));

  if (chartData.length === 0) {
    return (
      <div
        className="flex items-center justify-center text-sm text-muted-foreground"
        style={{ height }}
      >
        {t('charts.noData')}
      </div>
    );
  }

  return (
    <ResponsiveContainer width="100%" height={height}>
      <LineChart
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
        <Tooltip content={<ChartTooltip />} />
        <Line
          type="monotone"
          dataKey="avg"
          stroke="var(--primary)"
          strokeWidth={2}
          dot={false}
          isAnimationActive={false}
        />
        <Line
          type="monotone"
          dataKey="p95"
          stroke="var(--chart-2)"
          strokeWidth={2}
          strokeDasharray="4 3"
          dot={false}
          isAnimationActive={false}
        />
      </LineChart>
    </ResponsiveContainer>
  );
}
