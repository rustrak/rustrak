import type { MonitorStatsBucket } from '@rustrak/client';
import { cn, Tooltip } from '@rustrak/ui';
import { useFormatter, useTranslations } from 'use-intl';
import { checkInFill } from '@/features/monitor/lib/check-in-fill';
import { formatRunDuration } from '@/features/monitor/lib/duration';
import { tickStatus } from '@/features/monitor/lib/timeline';

const COUNTED = ['ok', 'error', 'timeout', 'missed', 'in_progress'] as const;

/**
 * One environment over the window, a tick per bucket on an axis every row
 * shares -- so "everything failed at 03:00" reads straight down the list.
 * A tick shows the worst run of its bucket; an empty one stays neutral.
 */
export function CheckInStrip({
  buckets,
  size = 'sm',
  label,
}: {
  buckets: MonitorStatsBucket[];
  size?: 'sm' | 'lg';
  /** Names the strip for screen readers: its monitor and environment. */
  label: string;
}) {
  const t = useTranslations('crons');
  const format = useFormatter();
  const width =
    buckets.length > 1
      ? Date.parse(buckets[1].bucket) - Date.parse(buckets[0].bucket)
      : 0;
  const daily = width >= 86_400_000;
  const when = (at: number) =>
    format.dateTime(new Date(at), daily ? 'axisDay' : 'axisTime');

  return (
    <ol
      aria-label={label}
      className={cn(
        'flex w-full min-w-0 items-stretch',
        size === 'lg' ? 'h-8 gap-0.75' : 'h-5 gap-px',
      )}
    >
      {buckets.map((bucket) => {
        const status = tickStatus(bucket);
        const start = Date.parse(bucket.bucket);
        const counts = COUNTED.filter((s) => bucket[s] > 0)
          .map((s) => `${bucket[s]} ${t(`status.${s}`).toLowerCase()}`)
          .join(' · ');
        const summary = `${when(start)} – ${when(start + width)}: ${counts || t('timeline.noRuns')}`;
        return (
          <Tooltip
            key={bucket.bucket}
            delay={0}
            content={
              <span className="flex flex-col gap-0.5">
                <span>
                  {when(start)} – {when(start + width)}
                </span>
                <span className="text-fg-subtle">
                  {counts || t('timeline.noRuns')}
                  {bucket.avg_duration_ms !== null
                    ? ` · ${t('timeline.avg', { duration: formatRunDuration(bucket.avg_duration_ms) })}`
                    : null}
                </span>
              </span>
            }
          >
            <li
              aria-label={summary}
              className={cn(
                'min-w-0 flex-1 rounded-xs',
                checkInFill(status),
                status === 'empty' && 'opacity-60',
              )}
            />
          </Tooltip>
        );
      })}
    </ol>
  );
}

/** The ends of a strip's axis, under or over it: where it starts, and now. */
export function StripAxis({
  buckets,
  className,
}: {
  buckets: MonitorStatsBucket[];
  className?: string;
}) {
  const t = useTranslations('crons');
  const format = useFormatter();
  const first = buckets[0];
  if (!first) return null;
  const start = new Date(first.bucket);
  const long = Date.now() - start.getTime() > 2 * 86_400_000;

  return (
    <div
      className={cn(
        'flex justify-between font-mono text-fg-subtle text-mono-sm tabular-nums',
        className,
      )}
    >
      <span>{format.dateTime(start, long ? 'axisDay' : 'clock')}</span>
      <span>{t('timeline.now')}</span>
    </div>
  );
}
