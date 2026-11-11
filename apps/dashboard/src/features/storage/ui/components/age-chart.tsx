import { ChartLegend, type ChartSeries, ChartTooltip } from '@rustrak/ui';
import { useMemo } from 'react';
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
import { useTranslations } from 'use-intl';
import {
  type AgeBucket,
  KINDS,
  type Kind,
} from '@/features/storage/model/retention';
import { formatBytes } from '@/shared/lib/utils';

/** Fixed categorical slots, in order: the colour follows the kind. */
const COLOR: Record<Kind, string> = {
  events: 'var(--chart-1)',
  transactions: 'var(--chart-2)',
  logs: 'var(--chart-3)',
};

const HEIGHT = 260;

/** An axis label: furniture, in the meta ink and the small mono. */
function AxisTick({
  x,
  y,
  payload,
  anchor,
  format,
}: {
  x?: number;
  y?: number;
  payload?: { value: string | number };
  anchor: 'middle' | 'end';
  format?: (value: string | number) => string;
}) {
  if (payload == null) return null;
  return (
    <text
      x={x}
      y={y}
      dy={anchor === 'middle' ? 12 : 3}
      textAnchor={anchor}
      className="fill-fg-meta font-mono text-mono-sm tabular-nums"
    >
      {format ? format(payload.value) : payload.value}
    </text>
  );
}

/**
 * Bytes by how long ago they arrived, stacked by kind. What the next cleanup
 * deletes is the hatched part of each bar, so the chart answers "what is
 * big, how old is it, and what goes" at once.
 */
export function AgeChart({ buckets }: { buckets: readonly AgeBucket[] }) {
  const t = useTranslations('storage');
  const kinds = KINDS;
  const series: ChartSeries[] = kinds.map((k) => ({
    key: k,
    label: t(`kind.${k}`),
    color: COLOR[k],
  }));
  const labels = buckets.map((_, i) => t(`age.bucket${i}`));
  const data = useMemo(
    () =>
      buckets.map((b, i) => {
        const row: Record<string, number | string> = { label: labels[i] ?? '' };
        for (const kind of kinds) {
          row[`${kind}Kept`] = b[kind] - b.goes[kind];
          row[`${kind}Goes`] = b.goes[kind];
        }
        return row;
      }),
    [buckets, kinds, labels],
  );
  const anyGoes = buckets.some((b) => kinds.some((k) => b.goes[k] > 0));
  const empty = !buckets.some((b) => kinds.some((k) => b[k] > 0));
  const Tip = useMemo(
    () => makeTooltip(buckets, labels, series, t('age.goes')),
    [buckets, labels, series, t],
  );

  if (empty) {
    return (
      <div
        className="flex items-center justify-center text-meta text-fg-subtle"
        style={{ height: HEIGHT }}
      >
        {t('age.empty')}
      </div>
    );
  }

  return (
    <div className="flex min-w-0 flex-col gap-3">
      <ResponsiveContainer width="100%" height={HEIGHT} debounce={80}>
        <BarChart data={data} margin={{ top: 8, right: 4, bottom: 0, left: 0 }}>
          <defs>
            {KINDS.map((kind) => (
              <pattern
                key={kind}
                id={`storage-hatch-${kind}`}
                width="6"
                height="6"
                patternUnits="userSpaceOnUse"
                patternTransform="rotate(45)"
              >
                <rect width="6" height="6" fill={COLOR[kind]} opacity={0.25} />
                <line
                  x1="0"
                  y1="0"
                  x2="0"
                  y2="6"
                  stroke={COLOR[kind]}
                  strokeWidth="3"
                />
              </pattern>
            ))}
          </defs>
          <CartesianGrid
            vertical={false}
            stroke="var(--border-divider)"
            strokeDasharray="3 3"
          />
          <XAxis
            dataKey="label"
            tick={<AxisTick anchor="middle" />}
            axisLine={false}
            tickLine={false}
            // Drops the labels that would overlap on a narrow screen; the
            // tooltip names every bucket.
            interval="preserveStartEnd"
            minTickGap={8}
          />
          <YAxis
            tick={
              <AxisTick anchor="end" format={(v) => formatBytes(Number(v))} />
            }
            axisLine={false}
            tickLine={false}
            width={76}
          />
          <Tooltip
            cursor={{ fill: 'var(--surface-hover)' }}
            content={<Tip />}
          />
          {kinds.flatMap((kind) => [
            <Bar
              key={`${kind}Kept`}
              dataKey={`${kind}Kept`}
              stackId="age"
              fill={COLOR[kind]}
              stroke="var(--surface)"
              strokeWidth={1}
              maxBarSize={56}
              isAnimationActive={false}
            />,
            <Bar
              key={`${kind}Goes`}
              dataKey={`${kind}Goes`}
              stackId="age"
              fill={`url(#storage-hatch-${kind})`}
              stroke="var(--surface)"
              strokeWidth={1}
              maxBarSize={56}
              isAnimationActive={false}
            />,
          ])}
        </BarChart>
      </ResponsiveContainer>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <ChartLegend series={series} />
        {anyGoes && (
          <span className="flex items-center gap-1.5 text-hint text-fg-subtle">
            {/* Neutral: the hatch means "goes" whatever the kind. */}
            <svg aria-hidden="true" viewBox="0 0 10 10" className="size-2.5">
              <defs>
                <pattern
                  id="storage-hatch-legend"
                  width="3.5"
                  height="3.5"
                  patternUnits="userSpaceOnUse"
                  patternTransform="rotate(45)"
                >
                  <line
                    x1="0"
                    y1="0"
                    x2="0"
                    y2="3.5"
                    stroke="var(--fg-meta)"
                    strokeWidth="1.5"
                  />
                </pattern>
              </defs>
              <rect
                width="10"
                height="10"
                rx="2"
                fill="url(#storage-hatch-legend)"
              />
            </svg>
            {t('age.goes')}
          </span>
        )}
      </div>
      {/* The bars as a table, for screen readers. */}
      {/* In a box: a table keeps its content width even when sr-only. */}
      <div className="sr-only">
        <table>
          <caption>{t('age.title')}</caption>
          <thead>
            <tr>
              <th>{t('age.age')}</th>
              {kinds.map((k) => (
                <th key={k}>{t(`kind.${k}`)}</th>
              ))}
              <th>{t('age.goes')}</th>
            </tr>
          </thead>
          <tbody>
            {buckets.map((b, i) => (
              <tr key={labels[i]}>
                <td>{labels[i]}</td>
                {kinds.map((k) => (
                  <td key={k}>{formatBytes(b[k])}</td>
                ))}
                <td>{formatBytes(kinds.reduce((s, k) => s + b.goes[k], 0))}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

/**
 * Per kind, the bucket's whole size rather than its kept and going halves as
 * two rows each; what goes is one line under them.
 */
function makeTooltip(
  buckets: readonly AgeBucket[],
  labels: readonly string[],
  series: ChartSeries[],
  goesLabel: string,
) {
  return function AgeTooltip(props: {
    active?: boolean;
    payload?: Array<{ payload: { label: string } }>;
  }) {
    const { active, payload } = props;
    const label = payload?.[0]?.payload.label ?? '';
    const bucket = buckets[labels.indexOf(label)];
    if (!active || !bucket) return null;
    const goes = KINDS.reduce((s, k) => s + bucket.goes[k], 0);
    const rows = [
      ...(goes > 0
        ? [{ dataKey: 'goes', value: goes, color: 'var(--fg-meta)' }]
        : []),
      ...[...KINDS]
        .reverse()
        .map((k) => ({ dataKey: k, value: bucket[k], color: COLOR[k] })),
    ];
    return (
      <ChartTooltip
        active
        label={label}
        payload={rows}
        series={[...series, { key: 'goes', label: goesLabel }]}
        formatValue={formatBytes}
      />
    );
  };
}
