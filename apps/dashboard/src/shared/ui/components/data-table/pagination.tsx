import {
  Button,
  ChevronLeftIcon,
  ChevronRightIcon,
  FirstPageIcon,
  LastPageIcon,
} from '@rustrak/ui';
import type { RowData } from '@tanstack/react-table';
import { useFormatter, useTranslations } from 'use-intl';
import type { DataTableInstance } from './use-app-table';

/**
 * The footer of a paginated table: what is on screen, and the way to the rest.
 *
 * Reads the table rather than taking counts, so the range and the controls
 * cannot disagree with the rows above them. Every table here is
 * `manualPagination`, so `getPageCount()` is derived from the `rowCount` the
 * server reported.
 *
 * Renders nothing when there is nothing to page through. A single page of
 * results does not need "Page 1 of 1" and two dead arrows under it.
 */
export function DataTablePagination<TData extends RowData>({
  table,
  disabled = false,
}: {
  table: DataTableInstance<TData>;
  disabled?: boolean;
}) {
  const format = useFormatter();
  const { pageIndex, pageSize } = table.state.pagination;
  const pageCount = table.getPageCount();
  const rowCount = table.getRowCount();
  const t = useTranslations('table');

  if (pageCount <= 1) return null;

  const first = pageIndex * pageSize + 1;
  const last = Math.min((pageIndex + 1) * pageSize, rowCount);

  return (
    <div className="flex shrink-0 flex-col items-center justify-between gap-2 pt-3 sm:flex-row">
      {/* No empty case to handle: `pageCount <= 1` returned above, and under
          `manualPagination` no rows means no pages. An empty list says so
          through the shell's `empty` slot, where the message can be about what
          was being looked for. */}
      <p className="text-fg-subtle text-meta tabular-nums">
        <span className="font-medium text-fg">
          {format.number(first)}-{format.number(last)}
        </span>{' '}
        {t('ofTotal', { total: format.number(rowCount) })}
      </p>

      <div className="flex items-center gap-1">
        <Button
          variant="secondary"
          size="sm"
          icon={FirstPageIcon}
          aria-label={t('firstPage')}
          onClick={() => table.setPageIndex(0)}
          disabled={disabled || !table.getCanPreviousPage()}
        />
        <Button
          variant="secondary"
          size="sm"
          icon={ChevronLeftIcon}
          aria-label={t('previousPage')}
          onClick={() => table.previousPage()}
          disabled={disabled || !table.getCanPreviousPage()}
        />

        <span className="px-2 text-fg-subtle text-meta tabular-nums">
          {t('pageOf', {
            current: pageIndex + 1,
            total: format.number(pageCount),
          })}
        </span>

        <Button
          variant="secondary"
          size="sm"
          icon={ChevronRightIcon}
          aria-label={t('nextPage')}
          onClick={() => table.nextPage()}
          disabled={disabled || !table.getCanNextPage()}
        />
        <Button
          variant="secondary"
          size="sm"
          icon={LastPageIcon}
          aria-label={t('lastPage')}
          onClick={() => table.setPageIndex(pageCount - 1)}
          disabled={disabled || !table.getCanNextPage()}
        />
      </div>
    </div>
  );
}
