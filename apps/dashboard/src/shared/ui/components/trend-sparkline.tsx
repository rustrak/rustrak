interface TrendSparklineProps {
  /** Bucketed counts, oldest first. */
  trend: number[];
  /**
   * Floor for the y-axis maximum.
   *
   * Without it every series is scaled to its own peak, so three events across
   * a quiet day render as a full-height bar that reads exactly like an
   * outage. Sentry hardcodes the same floor in its project chart with the
   * comment "this keeps small datasets from looking 'scary'" — worth more on
   * a self-hosted instance, where low volume is the normal case rather than
   * the exception.
   */
  minScale?: number;
  /** Accessible name; describe the window, e.g. "24h event trend". */
  label?: string;
  className?: string;
  /** Fill for the bars. Lets a caller encode a status in the colour. */
  barClassName?: string;
}

/** Compact bar sparkline for a table's trend column. */
export function TrendSparkline({
  trend,
  minScale = 1,
  label = '24h event trend',
  className = 'w-16 h-6 shrink-0',
  barClassName = 'fill-fg-subtle/50',
}: TrendSparklineProps) {
  if (trend.length === 0) {
    return null;
  }

  const max = Math.max(...trend, minScale);
  const barWidth = 2;
  const gap = 1;

  return (
    <svg
      viewBox={`0 0 ${trend.length * (barWidth + gap)} 24`}
      className={className}
      preserveAspectRatio="none"
      role="img"
      aria-label={label}
    >
      {trend.map((count, i) => {
        const height = count > 0 ? Math.max((count / max) * 22, 2) : 0;
        return (
          <rect
            key={i}
            x={i * (barWidth + gap)}
            y={24 - height}
            width={barWidth}
            height={height}
            rx={0.5}
            className={barClassName}
          />
        );
      })}
    </svg>
  );
}
