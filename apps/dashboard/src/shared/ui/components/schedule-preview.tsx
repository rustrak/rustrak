import type { Schedule } from '@rustrak/client';
import { CalendarClock, Info, Loader2 } from 'lucide-react';
import { useFormatter, useTranslations } from 'use-intl';
import { useSchedulePreview } from '@/shared/ui/hooks/use-schedule-preview';
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
    <div className="rounded-lg border bg-card">
      <div className="flex items-center gap-2 border-b px-4 py-3">
        <CalendarClock className="size-4 text-muted-foreground" />
        <h2 className="text-sm font-semibold">{t('preview.title')}</h2>
        {preview.pending && (
          <Loader2 className="ml-auto size-3.5 animate-spin text-muted-foreground" />
        )}
      </div>
      {preview.invalid ? (
        <p className="px-4 py-6 text-sm text-destructive">
          {t('preview.invalid')}
        </p>
      ) : runs.length === 0 || schedule === null ? (
        <p className="px-4 py-6 text-sm text-muted-foreground">
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
                      ? 'size-1.5 rounded-full bg-primary'
                      : 'size-1.5 rounded-full bg-muted-foreground/40'
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
          <p className="flex gap-1.5 border-t pt-3 text-xs text-muted-foreground">
            <Info className="mt-0.5 size-3 shrink-0" />
            {note ?? t('preview.timesIn', { timezone })}
          </p>
        </div>
      )}
    </div>
  );
}
