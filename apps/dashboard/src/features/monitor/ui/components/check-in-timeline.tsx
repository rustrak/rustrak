import type { CheckIn } from '@rustrak/client';
import { useFormatter, useTranslations } from 'use-intl';
import { formatRunDuration } from '@/features/monitor/lib/duration';
import { cn } from '@/shared/lib/utils';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/shared/ui/components/shadcn/tooltip';
import { checkInFill } from './check-in-status';

/**
 * The last runs of one environment, oldest on the left, one tick per
 * check-in.
 *
 * A fixed number of slots, padded on the left, so every row of the list
 * lines up and a monitor with three runs does not look like one with thirty.
 */
export function CheckInTimeline({
  checkIns,
  slots = 30,
  size = 'sm',
}: {
  /** Newest first, as the API returns them. */
  checkIns: CheckIn[];
  slots?: number;
  size?: 'sm' | 'lg';
}) {
  const t = useTranslations('crons');
  const format = useFormatter();
  const shown = checkIns.slice(0, slots).reverse();
  const tick =
    size === 'lg'
      ? 'h-9 flex-1 min-w-1 max-w-3 rounded-[3px]'
      : 'h-5 w-1.5 rounded-sm';

  return (
    <TooltipProvider delay={0}>
      <ol
        className={cn('flex items-end', size === 'lg' ? 'gap-1' : 'gap-[3px]')}
        aria-label={t('timeline.label')}
      >
        {Array.from({ length: slots - shown.length }, (_, i) => (
          <li
            key={`pad-${i}`}
            aria-hidden="true"
            className={cn(tick, 'bg-muted')}
          />
        ))}
        {shown.map((checkIn) => {
          const when = format.dateTime(
            new Date(checkIn.date_added),
            'dateTime',
          );
          return (
            <Tooltip key={checkIn.id}>
              <TooltipTrigger
                render={
                  <li
                    aria-label={`${t(`status.${checkIn.status}`)}, ${when}`}
                    className={cn(tick, checkInFill(checkIn.status))}
                  />
                }
              />
              <TooltipContent>
                <div className="flex flex-col gap-0.5 py-0.5">
                  <span className="font-semibold">
                    {t(`status.${checkIn.status}`)}
                  </span>
                  <span className="opacity-80">{when}</span>
                  {checkIn.duration_ms !== null && (
                    <span className="opacity-80">
                      {t('timeline.took', {
                        duration: formatRunDuration(checkIn.duration_ms),
                      })}
                    </span>
                  )}
                  {checkIn.environment !== 'production' && (
                    <span className="opacity-80">{checkIn.environment}</span>
                  )}
                </div>
              </TooltipContent>
            </Tooltip>
          );
        })}
      </ol>
    </TooltipProvider>
  );
}
