import type { CheckIn, OffsetPaginatedResponse } from '@rustrak/client';
import { useFormatter, useTranslations } from 'use-intl';
import { formatRunDuration } from '@/features/monitor/lib/duration';
import { TablePagination } from '@/shared/ui/components/table-pagination';
import { CheckInStatusLabel } from './check-in-status';

/** One monitor's check-ins, newest first, a page at a time. */
export function CheckInsTable({
  page,
  currentPage,
  showEnvironment,
  pending,
  onPageChange,
}: {
  page: OffsetPaginatedResponse<CheckIn>;
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
      <p className="rounded-lg border border-dashed px-4 py-8 text-center text-sm text-muted-foreground">
        {t('detail.noCheckIns')}
      </p>
    );
  }

  return (
    <div>
      <div className="overflow-x-auto rounded-lg border">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b bg-muted/50 text-left text-xs font-bold uppercase tracking-widest text-muted-foreground whitespace-nowrap">
              <th className="px-4 py-3 font-bold">
                {t('checkInColumns.status')}
              </th>
              {showEnvironment && (
                <th className="px-4 py-3 font-bold">
                  {t('checkInColumns.environment')}
                </th>
              )}
              <th className="px-4 py-3 font-bold">
                {t('checkInColumns.started')}
              </th>
              <th className="px-4 py-3 text-right font-bold">
                {t('checkInColumns.duration')}
              </th>
              <th className="hidden px-4 py-3 font-bold md:table-cell">
                {t('checkInColumns.expected')}
              </th>
            </tr>
          </thead>
          <tbody className="divide-y">
            {page.items.map((checkIn) => (
              <tr key={checkIn.id} className="hover:bg-muted/30">
                <td className="px-4 py-2.5 whitespace-nowrap">
                  <CheckInStatusLabel status={checkIn.status} />
                </td>
                {showEnvironment && (
                  <td className="px-4 py-2.5 text-muted-foreground">
                    {checkIn.environment}
                  </td>
                )}
                <td className="px-4 py-2.5 whitespace-nowrap tabular-nums">
                  {format.dateTime(new Date(checkIn.date_added), 'precise')}
                </td>
                <td className="px-4 py-2.5 text-right font-mono tabular-nums text-muted-foreground">
                  {checkIn.duration_ms === null
                    ? '—'
                    : formatRunDuration(checkIn.duration_ms)}
                </td>
                <td className="hidden px-4 py-2.5 whitespace-nowrap tabular-nums text-muted-foreground md:table-cell">
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
      <TablePagination
        currentPage={currentPage}
        totalPages={page.total_pages}
        totalCount={page.total_count}
        perPage={page.per_page}
        disabled={pending}
        onPageChange={onPageChange}
      />
    </div>
  );
}
