import type { ReactNode } from 'react';
import { tv } from '../../lib/tv';
import { type ChartSeries, seriesColor } from './chart-series';

/**
 * What every chart shares: the tooltip card, the legend row and the axis
 * ticks. The series contract and the palette order live in `chart-series.ts`.
 */

/* --- Axis ticks ----------------------------------------------------------- */

/*
 * Recharts hands tick renderers x/y plus the value. The text wears text
 * tokens -- mono, meta ink -- because an axis is furniture: it has to be
 * legible and forgettable at once.
 */
interface TickProps {
  x?: number;
  y?: number;
  payload?: { value: string | number };
  format?: (value: string | number) => string;
}

export function XTick({ x, y, payload, format }: TickProps) {
  if (payload == null) return null;
  return (
    <text
      x={x}
      y={y}
      dy={12}
      textAnchor="middle"
      className="fill-fg-ghost font-mono text-mono-sm"
    >
      {format ? format(payload.value) : payload.value}
    </text>
  );
}

XTick.displayName = 'XTick';

export function YTick({ x, y, payload, format }: TickProps) {
  if (payload == null) return null;
  return (
    <text
      x={x}
      y={y}
      dy={3}
      textAnchor="end"
      // `width="auto"` measures only nodes carrying recharts' own tick class;
      // without it the axis is sized to 0 and every label falls off the left.
      className="recharts-cartesian-axis-tick-value fill-fg-ghost font-mono text-mono-sm tabular-nums"
    >
      {format ? format(payload.value) : payload.value}
    </text>
  );
}

YTick.displayName = 'YTick';

/* --- Tooltip -------------------------------------------------------------- */

const tooltipCard = tv({
  slots: {
    card: [
      'min-w-36 rounded-md border border-border bg-surface-floating',
      'px-2.5 py-2 shadow-overlay',
    ],
    label: 'pb-1.5 font-mono text-fg-subtle text-mono-sm',
    row: 'flex items-center gap-2 py-0.5',
    dot: 'size-dot shrink-0 rounded-pill',
    name: 'min-w-0 flex-1 truncate text-fg-muted text-meta',
    value: 'shrink-0 font-mono text-fg text-mono-sm tabular-nums',
  },
});

const tipStyles = tooltipCard();

/** The shape recharts passes to a custom tooltip `content`. */
export interface ChartTooltipProps {
  active?: boolean;
  label?: string | number;
  payload?: Array<{
    dataKey?: string | number;
    value?: number | string;
    color?: string;
  }>;
  series: ChartSeries[];
  formatLabel?: (value: string | number) => string;
  formatValue?: (value: number) => string;
}

/**
 * The reading layer. Values live here and in the axis, never printed on
 * every mark: the chart shows the shape, the pointer asks for the number.
 */
export function ChartTooltip({
  active,
  label,
  payload,
  series,
  formatLabel,
  formatValue,
}: ChartTooltipProps) {
  if (!active || !payload?.length) return null;

  return (
    <div role="status" className={tipStyles.card()}>
      {label != null ? (
        <div className={tipStyles.label()}>
          {formatLabel ? formatLabel(label) : label}
        </div>
      ) : null}
      {/* Stacks paint bottom-up but read top-down: reverse to match the
          drawing, so the first row is the topmost band. */}
      {[...payload].reverse().map((entry) => {
        const meta = series.find((s) => s.key === entry.dataKey);
        return (
          <div key={String(entry.dataKey)} className={tipStyles.row()}>
            <span
              aria-hidden="true"
              className={tipStyles.dot()}
              style={{ background: entry.color }}
            />
            <span className={tipStyles.name()}>
              {meta?.label ?? entry.dataKey}
            </span>
            <span className={tipStyles.value()}>
              {typeof entry.value === 'number' && formatValue
                ? formatValue(entry.value)
                : entry.value}
            </span>
          </div>
        );
      })}
    </div>
  );
}

ChartTooltip.displayName = 'ChartTooltip';

/* --- Legend --------------------------------------------------------------- */

const legend = tv({
  slots: {
    row: 'flex flex-wrap items-center gap-x-4 gap-y-1 pt-2',
    item: 'flex items-center gap-1.5',
    dot: 'size-dot shrink-0 rounded-pill',
    label: 'text-fg-muted text-meta',
  },
});

const legendStyles = legend();

/**
 * Drawn for two series or more, never for one -- a lone series is named by
 * the card's title, and a one-item legend is a caption pretending to be a
 * key. Ours, not recharts': the text has to wear the system's tokens.
 */
export function ChartLegend({
  series,
  children,
}: {
  series: ChartSeries[];
  children?: ReactNode;
}) {
  if (series.length < 2) return null;

  return (
    <div className={legendStyles.row()}>
      {series.map((entry, index) => (
        <span key={entry.key} className={legendStyles.item()}>
          <span
            aria-hidden="true"
            className={legendStyles.dot()}
            style={{ background: seriesColor(entry, index) }}
          />
          <span className={legendStyles.label()}>{entry.label}</span>
        </span>
      ))}
      {children}
    </div>
  );
}

ChartLegend.displayName = 'ChartLegend';
