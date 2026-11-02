import type { MonitorsListCheckinsResponse } from '@rustrak/client';
import {
  Button,
  ChevronLeftIcon,
  ChevronRightIcon,
  cn,
  Text,
} from '@rustrak/ui';
import { useFormatter, useTranslations } from 'use-intl';
import { formatRunDuration } from '@/features/monitor/lib/duration';
import { CheckInStatusLabel } from './check-in-status';

/**
 * One monitor's check-ins, newest first, a page at a time: how each run
 * ended, when it started against when it was due, and how long it took.
 */
export function CheckInsTable({
  page,
  currentPage,
  showEnvironment,
  pending,
  onPageChange,
}: {
  page: MonitorsListCheckinsResponse;
  currentPage: number;
  /** Only worth a column when the monitor runs in more than one. */
  showEnvironment: boolean;
  pending: boolean;
  onPageChange: (page: number) => void;
}) {
  const t = useTranslations('crons');
  const format = useFormatter();

  if (page.total_count === 0) {
    return (
      <Text variant="meta" tone="subtle" className="px-5 py-6">
        {t('detail.noCheckIns')}
      </Text>
    );
  }

  const cell = 'px-5 py-2 whitespace-nowrap';

  return (
    <div className={cn('flex flex-col', pending && 'opacity-60')}>
      <div className="overflow-x-auto">
        <table className="w-full text-body">
          <thead>
            <tr className="border-border-divider border-y text-left text-column text-fg-meta">
              <th className={cn(cell, 'font-normal')}>
                {t('checkInColumns.status')}
              </th>
              {showEnvironment ? (
                <th className={cn(cell, 'font-normal')}>
                  {t('checkInColumns.environment')}
                </th>
              ) : null}
              <th className={cn(cell, 'font-normal')}>
                {t('checkInColumns.started')}
              </th>
              <th className={cn(cell, 'text-right font-normal')}>
                {t('checkInColumns.duration')}
              </th>
              <th className={cn(cell, 'hidden font-normal md:table-cell')}>
                {t('checkInColumns.expected')}
              </th>
            </tr>
          </thead>
          <tbody className="divide-y divide-border-divider">
            {page.items.map((checkIn) => (
              <tr key={checkIn.id}>
                <td className={cn(cell, 'text-fg')}>
                  <CheckInStatusLabel status={checkIn.status} />
                </td>
                {showEnvironment ? (
                  <td
                    className={cn(
                      cell,
                      'font-mono text-fg-secondary text-mono-sm',
                    )}
                  >
                    {checkIn.environment}
                  </td>
                ) : null}
                <td className={cn(cell, 'text-fg-secondary tabular-nums')}>
                  {format.dateTime(new Date(checkIn.date_added), 'precise')}
                </td>
                <td
                  className={cn(
                    cell,
                    'text-right font-mono text-fg-secondary text-mono-sm tabular-nums',
                  )}
                >
                  {checkIn.duration_ms == null
                    ? '—'
                    : formatRunDuration(checkIn.duration_ms)}
                </td>
                <td
                  className={cn(
                    cell,
                    'hidden text-fg-subtle tabular-nums md:table-cell',
                  )}
                >
                  {checkIn.expected_time
                    ? format.dateTime(
                        new Date(checkIn.expected_time),
                        'dateTime',
                      )
                    : '—'}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {page.total_pages > 1 ? (
        <div className="flex items-center justify-between gap-2 border-border-divider border-t px-5 pt-3">
          <Text variant="meta" tone="subtle">
            {t('pageOf', { page: currentPage, total: page.total_pages })}
          </Text>
          <div className="flex gap-1">
            <Button
              size="sm"
              variant="ghost"
              icon={ChevronLeftIcon}
              aria-label={t('newer')}
              disabled={pending || currentPage <= 1}
              onClick={() => onPageChange(currentPage - 1)}
            />
            <Button
              size="sm"
              variant="ghost"
              icon={ChevronRightIcon}
              aria-label={t('older')}
              disabled={pending || currentPage >= page.total_pages}
              onClick={() => onPageChange(currentPage + 1)}
            />
          </div>
        </div>
      ) : null}
    </div>
  );
}
