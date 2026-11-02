import type { MetricDelta } from '@rustrak/client';
import { ArrowRightIcon, Text, TrendDownIcon, TrendUpIcon } from '@rustrak/ui';
import { useFormatter, useTranslations } from 'use-intl';
import { percentChange } from '@/shared/lib/chart-format';
import { deltaTone, type Polarity } from '@/shared/lib/metric-tone';
import { cn } from '@/shared/lib/utils';

interface StatTileProps {
  label: string;
  metric: MetricDelta;
  polarity: Polarity;
  /** Sub-label under the value, e.g. the backlog the rate feeds. */
  footnote?: string;
}

/**
 * A headline number with its period-over-period change. The KPI primitive of
 * the overview grid: a single current value plus a trend is a stat tile, not a
 * one-bar bar chart.
 */
export function StatTile({ label, metric, polarity, footnote }: StatTileProps) {
  const t = useTranslations('statTile');
  const format = useFormatter();
  const change = percentChange(metric.current, metric.previous);
  const Arrow =
    change === null || change === 0
      ? ArrowRightIcon
      : change > 0
        ? TrendUpIcon
        : TrendDownIcon;

  return (
    <div className="flex flex-col justify-between gap-3 rounded-xl border border-border-subtle bg-panel p-4">
      <Text variant="column" tone="subtle">
        {label}
      </Text>
      <div className="flex flex-col gap-1.5">
        {/* Proportional figures, not tabular: at this size tabular digits give
            every glyph the width of a zero and the number reads loose. Tabular
            is for columns that have to align. */}
        <Text
          variant="numeric-lg"
          render={<p />}
          className="leading-none normal-nums"
          title={format.number(metric.current)}
        >
          {format.number(metric.current, 'compact')}
        </Text>
        {change === null ? (
          <span className="text-fg-subtle text-meta">
            {metric.previous === null
              ? t('noPriorPeriod')
              : t('noPriorActivity')}
          </span>
        ) : (
          <span
            className={cn(
              'flex items-center gap-0.5 font-medium text-meta tabular-nums',
              deltaTone(change, polarity),
            )}
          >
            <Arrow className="size-3.5" aria-hidden />
            {format.number(change, 'percentChange')}
            <span className="font-normal text-fg-subtle">{t('vsPrev')}</span>
          </span>
        )}
        {footnote ? (
          <span className="text-fg-subtle text-meta">{footnote}</span>
        ) : null}
      </div>
    </div>
  );
}
