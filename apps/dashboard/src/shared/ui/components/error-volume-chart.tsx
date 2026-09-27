import type { EventTimeseries } from '@rustrak/client';
import { type ReactElement, useMemo } from 'react';
// Static: where this chart renders, it is the content of the page.
// react-doctor-disable-next-line react-doctor/prefer-dynamic-import
import {
  Bar,
  BarChart,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { useFormatter, useTranslations } from 'use-intl';
import {
  ChartLegend,
  ChartTooltipCaption,
  ChartTooltipRow,
  ChartTooltipSurface,
} from '@/shared/ui/components/chart-tooltip';

/**
 * Stack order, bottom to top. Errors sit on the baseline so the segment that
 * matters most can be read straight off the y-axis without mental arithmetic.
 */
const SERIES_KEYS = ['errors', 'warning', 'info'] as const;

type SeriesKey = (typeof SERIES_KEYS)[number];

interface ChartPoint {
  t: number;
  total: number;
  errors: number;
  /** Broken out for the tooltip only: fatal is folded into `errors` visually. */
  fatal: number;
  warning: number;
  info: number;
}

/**
 * Most bars to draw, whatever the window.
 *
 * Each bar costs three marks, so an all-time view of a year-old project would
 * otherwise be tens of thousands of DOM nodes, re-laid-out on every resize
 * tick. Adjacent buckets are merged to stay under this. It is also the more
 * readable chart: a bar narrower than a few pixels carries no information a
 * merged one does not.
 */
const MAX_BARS = 90;

/**
 * Above this many bars the surface gap and rounded cap stop being visible, so
 * the cheap built-in rect is used instead of the custom shape.
 */
const DETAILED_SHAPE_LIMIT = 60;

/** 2px of surface between touching segments, per the chart mark spec. */
const SEGMENT_GAP = 2;
/** Rounded data-end on the top of the stack; the baseline stays square. */
const CAP_RADIUS = 4;

interface SegmentProps {
  x?: number;
  y?: number;
  width?: number;
  height?: number;
  fill?: string;
  payload?: ChartPoint;
}

/** Rect with the two top corners rounded and the baseline square. */
function roundedTopRect(
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
): string {
  const r = Math.min(radius, width / 2, height);
  if (r <= 0) {
    return `M${x},${y}h${width}v${height}h${-width}Z`;
  }
  return [
    `M${x},${y + height}`,
    `V${y + r}`,
    `A${r},${r} 0 0 1 ${x + r},${y}`,
    `H${x + width - r}`,
    `A${r},${r} 0 0 1 ${x + width},${y + r}`,
    `V${y + height}`,
    'Z',
  ].join('');
}

/**
 * Builds the renderer for one stacked segment: the surface gap, plus the
 * rounded cap on the topmost non-empty segment.
 *
 * Recharts has no notion of a gap between stacked segments, so the separation
 * is cut out of each segment's own top edge rather than drawn as a stroke
 * around it (a stroke would add ink that is not data).
 *
 * Built once per series at module scope, never inside render: `shape` is a
 * component *type*, so a fresh function on each render makes React unmount and
 * remount every bar in the chart on every resize tick.
 */
function makeSegmentShape(dataKey: SeriesKey) {
  return function Segment({
    x = 0,
    y = 0,
    width = 0,
    height = 0,
    fill,
    payload,
  }: SegmentProps) {
    if (height <= 0 || width <= 0 || !payload) {
      return null;
    }

    const topMost = [...SERIES_KEYS].reverse().find((k) => payload[k] > 0);
    const isTop = topMost === dataKey;

    // Only inset segments that have something stacked above them, and only
    // when the segment is tall enough to survive it.
    const inset = !isTop && height > SEGMENT_GAP + 1 ? SEGMENT_GAP : 0;
    const radius = isTop ? CAP_RADIUS : 0;

    return (
      <path
        d={roundedTopRect(x, y + inset, width, height - inset, radius)}
        fill={fill}
      />
    );
  };
}

const SEGMENT_SHAPES: Record<
  SeriesKey,
  (props: SegmentProps) => ReactElement | null
> = {
  errors: makeSegmentShape('errors'),
  warning: makeSegmentShape('warning'),
  info: makeSegmentShape('info'),
};

/**
 * Merge every `groupSize` adjacent buckets into one bar.
 *
 * The merged bar keeps the timestamp of the first bucket in the group, which
 * is what the time axis and the tooltip both read as "the period starting
 * here".
 */
function mergeBuckets(points: ChartPoint[], groupSize: number): ChartPoint[] {
  if (groupSize <= 1) {
    return points;
  }

  const merged: ChartPoint[] = [];
  for (let i = 0; i < points.length; i += groupSize) {
    const group = points.slice(i, i + groupSize);
    merged.push({
      t: group[0].t,
      total: group.reduce((n, p) => n + p.total, 0),
      errors: group.reduce((n, p) => n + p.errors, 0),
      fatal: group.reduce((n, p) => n + p.fatal, 0),
      warning: group.reduce((n, p) => n + p.warning, 0),
      info: group.reduce((n, p) => n + p.info, 0),
    });
  }
  return merged;
}

/** Human-readable width of one bar, for the tooltip. */
function bucketLabel(
  ms: number,
  t: (key: string, values?: Record<string, string | number>) => string,
): string {
  const hours = Math.round(ms / 3_600_000);
  if (hours >= 48) {
    return t('bucketDays', { count: Math.round(hours / 24) });
  }
  if (hours >= 1) {
    return t('bucketHours', { count: hours });
  }
  return t('bucketMinutes', { count: Math.round(ms / 60_000) });
}

/**
 * The formatter is passed in, exactly like `t`.
 *
 * `useFormatter` is a hook, and this is a builder rather than a component, so
 * it cannot reach for one itself. The component that calls it already threads
 * the translator through for the same reason.
 */
type Formatter = ReturnType<typeof useFormatter>;

function makeTooltip(
  bucketMs: number | null,
  series: Array<{ key: SeriesKey; label: string; color: string }>,
  t: (key: string, values?: Record<string, string | number>) => string,
  format: Formatter,
) {
  return function ChartTooltip(props: {
    active?: boolean;
    payload?: Array<{ payload: ChartPoint }>;
  }) {
    const { active, payload } = props;
    if (!active || !payload?.length) {
      return null;
    }
    const point = payload[0].payload;

    return (
      <ChartTooltipSurface>
        <ChartTooltipRow
          label={t('total')}
          value={format.number(point.total)}
        />
        {series.map((s) => (
          <ChartTooltipRow
            key={s.key}
            color={s.color}
            label={s.label}
            value={format.number(point[s.key])}
          />
        ))}
        {point.fatal > 0 ? (
          // Fatal is stacked inside Errors, so the only place it is visible as
          // its own number is here.
          <ChartTooltipRow
            label={t('ofWhichFatal')}
            value={format.number(point.fatal)}
          />
        ) : null}
        <ChartTooltipCaption>
          {format.dateTime(new Date(point.t), 'dateTime')}
          {bucketMs ? ` · ${bucketLabel(bucketMs, t)}` : ''}
        </ChartTooltipCaption>
      </ChartTooltipSurface>
    );
  };
}

interface ErrorVolumeChartProps {
  data: EventTimeseries;
  height?: number;
}

/**
 * Error-event volume over time, stacked by severity. The lead chart of the
 * project overview.
 *
 * `fatal` folds into the Errors segment: it is an error, just a terminal one,
 * and as its own stacked band it sits too close to the Errors hue to be told
 * apart reliably. The tooltip keeps the exact count.
 */
export function ErrorVolumeChart({
  data,
  height = 260,
}: ErrorVolumeChartProps) {
  const format = useFormatter();
  const t = useTranslations('charts');

  const series = useMemo(
    (): Array<{ key: SeriesKey; label: string; color: string }> => [
      { key: 'errors', label: t('errors'), color: 'var(--sev-error)' },
      { key: 'warning', label: t('warnings'), color: 'var(--sev-warning)' },
      { key: 'info', label: t('info'), color: 'var(--sev-info)' },
    ],
    [t],
  );

  const { chartData, mean, bucketMs, detailedShape } = useMemo(() => {
    const raw: ChartPoint[] = data.map((point) => ({
      t: new Date(point.bucket).getTime(),
      total: point.total,
      errors: point.fatal + point.error,
      fatal: point.fatal,
      warning: point.warning,
      info: point.info,
    }));

    const groupSize = Math.max(1, Math.ceil(raw.length / MAX_BARS));
    const points = mergeBuckets(raw, groupSize);

    // Spacing between the first two bars is the bar width; a single bar has no
    // spacing to read, so the tooltip drops the bucket note rather than guess.
    const spacing = points.length > 1 ? points[1].t - points[0].t : null;

    return {
      chartData: points,
      mean: points.length
        ? points.reduce((sum, p) => sum + p.total, 0) / points.length
        : 0,
      bucketMs: groupSize > 1 ? spacing : null,
      detailedShape: points.length <= DETAILED_SHAPE_LIMIT,
    };
  }, [data]);

  const Tip = useMemo(
    () => makeTooltip(bucketMs, series, t, format),
    [bucketMs, series, t, format],
  );

  if (!chartData.some((p) => p.total > 0)) {
    return (
      <div
        className="flex items-center justify-center text-sm text-muted-foreground"
        style={{ height }}
      >
        {t('noEvents')}
      </div>
    );
  }

  return (
    <div className="flex min-w-0 flex-col gap-3">
      {/*
        debounce: collapsing the sidebar emits a continuous stream of resize
        observations, and every one of them re-lays-out the whole chart.
      */}
      <ResponsiveContainer width="100%" height={height} debounce={80}>
        <BarChart
          data={chartData}
          margin={{ top: 8, right: 4, bottom: 0, left: 0 }}
        >
          <XAxis
            dataKey="t"
            type="number"
            scale="time"
            domain={['dataMin', 'dataMax']}
            tickFormatter={(v) => format.dateTime(new Date(v), 'axisTime')}
            tick={{ fontSize: 11, fill: 'var(--muted-foreground)' }}
            axisLine={false}
            tickLine={false}
            minTickGap={64}
          />
          <YAxis
            tickFormatter={(v) => format.number(v, 'compact')}
            tick={{ fontSize: 11, fill: 'var(--muted-foreground)' }}
            axisLine={false}
            tickLine={false}
            width={44}
            allowDecimals={false}
          />
          <Tooltip
            cursor={{ fill: 'var(--muted)', opacity: 0.5 }}
            content={<Tip />}
          />
          {/* Says "is this bucket unusual?" without a second axis. */}
          <ReferenceLine
            y={mean}
            stroke="var(--muted-foreground)"
            strokeDasharray="4 4"
            strokeOpacity={0.6}
          />
          {SERIES_KEYS.map((key) => (
            <Bar
              key={key}
              dataKey={key}
              stackId="volume"
              fill={series.find((s) => s.key === key)?.color}
              maxBarSize={24}
              shape={detailedShape ? SEGMENT_SHAPES[key] : undefined}
              isAnimationActive={false}
            />
          ))}
        </BarChart>
      </ResponsiveContainer>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <ChartLegend items={series} />
        <span className="text-xs text-muted-foreground">
          {t('dashedLine', { count: Math.round(mean) })}
        </span>
      </div>
    </div>
  );
}
