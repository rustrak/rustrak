import type { MetricDelta } from '@rustrak/client';
import { ArrowRightIcon, TrendDownIcon, TrendUpIcon } from '@rustrak/ui';
import { useFormatter } from 'use-intl';
import { percentChange } from '@/shared/lib/chart-format';
import { deltaTone, type Polarity } from '@/shared/lib/metric-tone';
import { cn } from '@/shared/lib/utils';

interface MetricDeltaTextProps {
  metric: MetricDelta;
  polarity: Polarity;
}

/**
 * One-line period-over-period change, sized for a table cell.
 *
 * The tile-sized version of this lives in `StatTile`; this one drops the
 * "vs prev" caption, which a column header already implies.
 */
export function MetricDeltaText({ metric, polarity }: MetricDeltaTextProps) {
  const format = useFormatter();
  const change = percentChange(metric.current, metric.previous);

  if (change === null) {
    // An em dash would be wrong here: this is "nothing to compare against",
    // not "zero change", and the two must not look alike.
    return <span className="text-xs text-fg-subtle">&ndash;</span>;
  }

  const Arrow =
    change === 0 ? ArrowRightIcon : change > 0 ? TrendUpIcon : TrendDownIcon;

  return (
    <span
      className={cn(
        'inline-flex items-center gap-0.5 text-xs font-medium tabular-nums',
        deltaTone(change, polarity),
      )}
    >
      <Arrow className="size-3" aria-hidden />
      {format.number(change, 'percentChange')}
    </span>
  );
}
