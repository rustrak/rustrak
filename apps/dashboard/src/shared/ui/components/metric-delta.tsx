import type { MetricDelta } from '@rustrak/client';
import { ArrowRightIcon, TrendDownIcon, TrendUpIcon } from '@rustrak/ui';
import { useFormatter } from 'use-intl';
import { percentChange } from '@/shared/lib/chart-format';
import { cn } from '@/shared/lib/utils';

/**
 * One-line period-over-period change, sized for a table cell. Every metric it
 * shows today is one where a rise is bad news, so up is red. Nothing over all
 * time (no previous window); a dash when the previous window was zero.
 */
export function MetricDeltaText({ metric }: { metric: MetricDelta }) {
  const format = useFormatter();
  if (metric.previous === null) return null;
  const change = percentChange(metric.current, metric.previous);

  if (change === null) {
    // An en dash, not an em dash: "nothing to compare against" must not look
    // like "zero change".
    return <span className="text-xs text-fg-subtle">&ndash;</span>;
  }

  const Arrow =
    change === 0 ? ArrowRightIcon : change > 0 ? TrendUpIcon : TrendDownIcon;

  return (
    <span
      className={cn(
        'inline-flex items-center gap-0.5 text-xs font-medium tabular-nums',
        change === 0
          ? 'text-fg-subtle'
          : change > 0
            ? 'text-sev-error-fg'
            : 'text-success-fg',
      )}
    >
      <Arrow className="size-3" aria-hidden />
      {format.number(change, 'percentChange')}
    </span>
  );
}
