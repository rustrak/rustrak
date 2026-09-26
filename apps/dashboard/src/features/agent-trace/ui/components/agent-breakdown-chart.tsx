import type { GenAiBreakdownRow } from '@rustrak/client';
// Static: where this chart renders, it is the content of the page.
// react-doctor-disable-next-line react-doctor/prefer-dynamic-import
import {
  Bar,
  BarChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { useFormatter, useTranslations } from 'use-intl';

interface AgentBreakdownChartProps {
  rows: GenAiBreakdownRow[];
  formatValue?: (value: number) => string;
  height?: number;
}

function ChartTooltip({
  active,
  payload,
  formatValue,
}: {
  active?: boolean;
  payload?: Array<{ payload: { label: string; value: number } }>;
  formatValue: (value: number) => string;
}) {
  if (!active || !payload?.length) {
    return null;
  }
  const point = payload[0].payload;
  return (
    <div className="rounded-md border bg-popover px-2.5 py-1.5 text-xs shadow-md">
      <p className="font-medium text-popover-foreground">{point.label}</p>
      <p className="text-muted-foreground">{formatValue(point.value)}</p>
    </div>
  );
}

/**
 * A "top N by X" categorical bar chart — LLM Calls by Model / Tokens Used by
 * Model / Tool Calls by Tool widgets. Horizontal orientation reads better
 * for the typically-long model/tool name labels.
 */
export function AgentBreakdownChart({
  rows,
  formatValue,
  height = 130,
}: AgentBreakdownChartProps) {
  const t = useTranslations('agents');
  const format = useFormatter();
  // The default cannot be a module-scope constant any more: reading the
  // request locale means calling a hook, and a hook only runs inside the
  // component. `??` here rather than a default parameter for the same reason.
  const formatOne = formatValue ?? ((value: number) => format.number(value));

  if (rows.length === 0) {
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
      <BarChart
        data={rows}
        layout="vertical"
        margin={{ top: 6, right: 12, bottom: 0, left: 0 }}
      >
        <XAxis type="number" hide />
        <YAxis
          type="category"
          dataKey="label"
          width={110}
          tick={{ fontSize: 11, fill: 'var(--muted-foreground)' }}
          axisLine={false}
          tickLine={false}
          tickFormatter={(v: string) =>
            v.length > 16 ? `${v.slice(0, 15)}…` : v
          }
        />
        <Tooltip
          cursor={{ fill: 'var(--muted)', opacity: 0.5 }}
          content={<ChartTooltip formatValue={formatOne} />}
        />
        <Bar
          dataKey="value"
          fill="var(--chart-1)"
          radius={[0, 2, 2, 0]}
          maxBarSize={18}
          isAnimationActive={false}
        />
      </BarChart>
    </ResponsiveContainer>
  );
}
