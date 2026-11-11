import type { Schedule } from '@rustrak/client';
import { CronsIcon, InfoIcon, SpinnerIcon } from '@rustrak/ui';
import { useFormatter, useTranslations } from 'use-intl';
import { useSchedulePreview } from '@/features/monitor/ui/hooks/use-schedule-preview';
import { ScheduleText } from './schedule-text';

/**
 * The next runs of a schedule, in its zone, as the server will run them.
 * `null` means the schedule is still being typed.
 */
export function SchedulePreview({
  schedule,
  timezone,
  note,
}: {
  schedule: Schedule | null;
  timezone: string;
  /** A line under the runs, e.g. what counts as a missed run. */
  note?: string;
}) {
  const t = useTranslations('schedule');
  const format = useFormatter();
  const preview = useSchedulePreview(schedule, timezone);
  const runs = preview.runs ?? [];

  return (
    <div className="rounded-lg border bg-surface">
      <div className="flex items-center gap-2 border-b px-4 py-3">
        <CronsIcon className="size-4 text-fg-subtle" />
        <h2 className="text-sm font-semibold">{t('preview.title')}</h2>
        {preview.pending && (
          <SpinnerIcon className="ml-auto size-3.5 animate-spin text-fg-subtle" />
        )}
      </div>
      {preview.invalid ? (
        <p className="px-4 py-6 text-sm text-danger-fg">
          {t('preview.invalid')}
        </p>
      ) : runs.length === 0 || schedule === null ? (
        <p className="px-4 py-6 text-sm text-fg-subtle">
          {t(preview.failed ? 'preview.failed' : 'preview.empty')}
        </p>
      ) : (
        <div className="space-y-3 px-4 py-4">
          <p className="text-sm font-medium">
            <ScheduleText schedule={schedule} />
          </p>
          <ol className="space-y-1.5">
            {runs.map((run, index) => (
              <li
                key={run}
                className="flex items-center gap-2.5 text-sm tabular-nums"
              >
                <span
                  aria-hidden="true"
                  className={
                    index === 0
                      ? 'size-1.5 rounded-full bg-surface-brand'
                      : 'size-1.5 rounded-full bg-fg-subtle/40'
                  }
                />
                {format.dateTime(new Date(run), {
                  weekday: 'short',
                  month: 'short',
                  day: 'numeric',
                  hour: '2-digit',
                  minute: '2-digit',
                  timeZone: timezone,
                })}
              </li>
            ))}
          </ol>
          <p className="flex gap-1.5 border-t pt-3 text-xs text-fg-subtle">
            <InfoIcon className="mt-0.5 size-3 shrink-0" />
            {note ?? t('preview.timesIn', { timezone })}
          </p>
        </div>
      )}
    </div>
  );
}
