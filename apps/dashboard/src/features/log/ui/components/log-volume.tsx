import type { LogsVolumeResponse } from '@rustrak/client';
import { BarsChart } from '@rustrak/ui';
import { useFormatter, useTranslations } from 'use-intl';
import { volumeBars } from '@/features/log/model/log-view';
import { isDaily } from '@/shared/lib/chart-format';
import { useBucketLabel } from '@/shared/ui/hooks/use-bucket-label';

/**
 * How many lines the search matches over the window, stacked by level: where
 * the list's lines sit in time, and whether the errors came in one burst.
 */
export function LogVolume({ points }: { points: LogsVolumeResponse }) {
  const t = useTranslations('logs');
  const format = useFormatter();
  const bucketLabel = useBucketLabel(isDaily(points));

  return (
    <BarsChart
      data={volumeBars(points)}
      series={[
        { key: 'error', label: t('levels.error'), color: 'var(--sev-error)' },
        { key: 'warn', label: t('levels.warn'), color: 'var(--sev-warning)' },
        { key: 'info', label: t('levels.info'), color: 'var(--sev-info)' },
        { key: 'debug', label: t('levels.debug'), color: 'var(--chart-muted)' },
      ]}
      xKey="t"
      integer
      height={120}
      formatX={bucketLabel}
      formatY={(value) => format.number(value, 'compact')}
      label={t('volume')}
    />
  );
}
