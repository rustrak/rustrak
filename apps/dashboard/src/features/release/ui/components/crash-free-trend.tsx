import type { SessionTimeseries } from '@rustrak/client';
import { useMemo } from 'react';
// Static: where this chart renders, it is the content of the page.
// react-doctor-disable-next-line react-doctor/prefer-dynamic-import
import {
  Line,
  LineChart,
  ReferenceLine,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import { useFormatter, useTranslations } from 'use-intl';
import { crashFreeColor, pct } from '@/features/release/model/session-health';
import {
  ChartTooltipCaption,
  ChartTooltipRow,
  ChartTooltipSurface,
} from '@/shared/ui/components/chart-tooltip';

/** The rate every project is aiming at. Drawn as the line to beat. */
const TARGET = 0.99;

/**
 * Floor of the y-axis unless the data goes lower.
 *
 * Anchoring at zero would flatten every project into the same line pinned to
 * the top of the plot: the difference between a healthy 99.8% and an incident
 * at 94% is the whole story, and it lives in the last few percent.
 */
const DEFAULT_FLOOR = 0.9;

interface ChartPoint {
  t: number;
  rate: number | null;
  total: number;
  crashed: number;
}

/**
 * Draws a dot only where a measured bucket has no measured neighbour.
 *
 * The line is deliberately broken at nulls, which leaves an isolated
 * measurement with no segment to belong to: with `dot={false}` it would render
 * as nothing at all and the reader would lose a real data point. Dotting only
 * the isolated ones keeps the line clean everywhere else.
 */
function makeIsolatedDot(points: ChartPoint[]) {
  return function IsolatedDot(props: {
    cx?: number;
    cy?: number;
    index?: number;
    stroke?: string;
  }) {
    const { cx, cy, index, stroke } = props;
    if (cx === undefined || cy === undefined || index === undefined) {
      return null;
    }
    if (points[index]?.rate === null) {
      return null;
    }
    const isolated =
      (points[index - 1]?.rate ?? null) === null &&
      (points[index + 1]?.rate ?? null) === null;
    if (!isolated) {
      return null;
    }
    return <circle cx={cx} cy={cy} r={3} fill={stroke} />;
  };
}

function ChartTooltip(props: {
  active?: boolean;
  payload?: Array<{ payload: ChartPoint }>;
}) {
  const format = useFormatter();
  const t = useTranslations('releases');
  const { active, payload } = props;
  if (!active || !payload?.length) {
    return null;
  }
  const point = payload[0].payload;

  return (
    <ChartTooltipSurface>
      <ChartTooltipRow label={t('crashFree')} value={pct(point.rate)} />
      <ChartTooltipRow
        label={t('sessions')}
        value={format.number(point.total)}
      />
      <ChartTooltipRow
        label={t('crashed')}
        value={format.number(point.crashed)}
      />
      <ChartTooltipCaption>
        {format.dateTime(new Date(point.t), 'dateTime')}
      </ChartTooltipCaption>
    </ChartTooltipSurface>
  );
}

interface CrashFreeTrendProps {
  /** Crash-free sessions rate for the window, or null with no session data. */
  sessionsRate: number | null;
  /** Crash-free users rate for the window. */
  usersRate: number | null;
  /** Total sessions the rates are computed from. */
  totalSessions: number;
  /** Per-bucket rates behind the headline figure. */
  data: SessionTimeseries;
  height?: number;
}

/**
 * Crash-free rate: the headline figure for the window, next to how it got
 * there.
 *
 * The number alone cannot say whether a project is recovering or falling over,
 * which is the question the overview exists to answer, so the figure and the
 * trend ship together. One y-axis, one series: the users rate stays a
 * secondary figure rather than a second line on a second scale.
 */
export function CrashFreeTrend({
  sessionsRate,
  usersRate,
  totalSessions,
  data,
  height = 132,
}: CrashFreeTrendProps) {
  const format = useFormatter();
  const t = useTranslations('releases');
  const { chartData, floor } = useMemo(() => {
    const points: ChartPoint[] = data.map((point) => ({
      t: new Date(point.bucket).getTime(),
      rate: point.crash_free_sessions_rate,
      total: point.total,
      crashed: point.crashed,
    }));

    const rates = points
      .map((p) => p.rate)
      .filter((r): r is number => r !== null);

    return {
      chartData: points,
      // Let a bad window pull the floor down rather than clipping the dip
      // that made it interesting.
      floor: rates.length ? Math.min(DEFAULT_FLOOR, ...rates) : DEFAULT_FLOOR,
    };
  }, [data]);

  // Built once per dataset, not per render: `dot` is a component type, so a
  // fresh function each render remounts every marker. Declared before the
  // early return below, so the hook order never changes between renders.
  const IsolatedDot = useMemo(() => makeIsolatedDot(chartData), [chartData]);

  if (sessionsRate === null && usersRate === null) {
    return (
      <div
        className="flex flex-col items-center justify-center text-center"
        style={{ height }}
      >
        <p className="text-sm text-muted-foreground">{t('noSessionData')}</p>
        <p className="mt-1 text-xs text-muted-foreground/70">
          {t('enableReleaseHealth')}
        </p>
      </div>
    );
  }

  const color = crashFreeColor(sessionsRate);
  const hasTrend = chartData.some((p) => p.rate !== null);

  return (
    <div className="flex flex-wrap items-start gap-x-6 gap-y-4">
      <div className="shrink-0">
        {/* Proportional figures, not tabular: at display sizes tabular digits
            give every glyph the width of a zero and the number reads loose. */}
        <p className="text-4xl font-bold leading-none" style={{ color }}>
          {pct(sessionsRate)}
        </p>
        <dl className="mt-3 space-y-1.5 text-xs">
          <div className="flex items-baseline gap-2">
            <dt className="text-muted-foreground">{t('users')}</dt>
            <dd
              className="font-semibold"
              style={{ color: crashFreeColor(usersRate) }}
            >
              {pct(usersRate)}
            </dd>
          </div>
          <div className="flex items-baseline gap-2">
            <dt className="text-muted-foreground">{t('sessions')}</dt>
            <dd className="font-semibold tabular-nums">
              {format.number(totalSessions)}
            </dd>
          </div>
        </dl>
      </div>

      <div className="min-w-[12rem] flex-1">
        {hasTrend ? (
          <ResponsiveContainer width="100%" height={height} debounce={80}>
            <LineChart
              data={chartData}
              margin={{ top: 8, right: 4, bottom: 0, left: 0 }}
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
              <YAxis
                domain={[floor, 1]}
                tickFormatter={(v) => `${Math.round(v * 100)}%`}
                tick={{ fontSize: 11, fill: 'var(--muted-foreground)' }}
                axisLine={false}
                tickLine={false}
                width={40}
              />
              <Tooltip
                cursor={{ stroke: 'var(--muted-foreground)', strokeWidth: 1 }}
                content={<ChartTooltip />}
              />
              {/* The target, so the line is read against something rather
                  than against itself. */}
              <ReferenceLine
                y={TARGET}
                stroke="var(--muted-foreground)"
                strokeDasharray="4 4"
                strokeOpacity={0.6}
                label={{
                  value: '99%',
                  position: 'insideTopRight',
                  fontSize: 10,
                  fill: 'var(--muted-foreground)',
                }}
              />
              {/*
                No connectNulls. A bucket whose sessions all arrived as
                terminal updates with no `init` lands with total 0, and the
                rate comes back null (see the CASE in
                SessionService::session_timeseries). Bridging it would draw a
                confident line across a period where nothing was measured;
                breaking the line says so instead.
              */}
              <Line
                dataKey="rate"
                stroke={color}
                strokeWidth={2}
                strokeLinecap="round"
                strokeLinejoin="round"
                dot={IsolatedDot}
                activeDot={{ r: 4 }}
                isAnimationActive={false}
              />
            </LineChart>
          </ResponsiveContainer>
        ) : (
          <div
            className="flex items-center justify-center text-xs text-muted-foreground"
            style={{ height }}
          >
            {t('noTrendHistory')}
          </div>
        )}
      </div>
    </div>
  );
}
