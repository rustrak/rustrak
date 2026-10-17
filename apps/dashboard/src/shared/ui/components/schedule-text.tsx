import type { Schedule } from '@rustrak/client';
import { useLocale, useTranslations } from 'use-intl';
import { describeCrontab } from '@/shared/lib/crontab-description';

/**
 * A schedule in words: "At 03:00", "Every 2 hours". A crontab cronstrue
 * cannot read falls back to the raw pattern rather than to nothing.
 */
export function ScheduleText({
  schedule,
  timezone,
}: {
  schedule: Schedule;
  /** Appended unless it is UTC, the default nobody needs told. */
  timezone?: string;
}) {
  const t = useTranslations('schedule');
  const locale = useLocale();
  const text =
    schedule.type === 'crontab'
      ? (describeCrontab(schedule.value, locale) ?? schedule.value)
      : t(`intervalText.${schedule.unit}`, { count: schedule.value });

  return (
    <span>
      {text}
      {timezone && timezone !== 'UTC' && (
        <span className="text-muted-foreground"> · {timezone}</span>
      )}
    </span>
  );
}
