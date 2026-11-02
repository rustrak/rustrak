import { Checkbox, ChevronRightIcon } from '@rustrak/ui';
import type { RowData } from '@tanstack/react-table';
import { cn } from '@/shared/lib/utils';
import { StopPropagation } from '@/shared/ui/components/stop-propagation';
import { createAppColumnHelper } from './use-app-table';

/**
 * The tick column, for tables with batch actions.
 *
 * Fixed, unresizable and unhideable, because none of those are meaningful for
 * a 44px control, and provided here rather than written per table so that the
 * select-all semantics stay one decision. "All" means the rows on this page:
 * the table only ever holds one page, and a select-all reaching rows that were
 * never fetched is a claim it cannot honour.
 */
export function selectionColumn<TData extends RowData>(
  t: (key: string) => string,
) {
  const helper = createAppColumnHelper<TData>();
  return helper.display({
    id: 'select',
    size: 44,
    minSize: 44,
    maxSize: 44,
    header: ({ table }) => (
      <StopPropagation>
        <Checkbox
          aria-label={t('selectAllPageRows')}
          checked={table.getIsAllPageRowsSelected()}
          indeterminate={
            table.getIsSomePageRowsSelected() &&
            !table.getIsAllPageRowsSelected()
          }
          onCheckedChange={(checked) =>
            table.toggleAllPageRowsSelected(checked === true)
          }
        />
      </StopPropagation>
    ),
    cell: ({ row }) => (
      // Without this, ticking a row in a table whose rows expand or navigate
      // would do both.
      <StopPropagation>
        <Checkbox
          aria-label={t('selectRow')}
          checked={row.getIsSelected()}
          disabled={!row.getCanSelect()}
          onCheckedChange={(checked) => row.toggleSelected(checked === true)}
        />
      </StopPropagation>
    ),
  });
}

/**
 * The disclosure chevron, for tables whose rows open a detail panel.
 *
 * Deliberately not a button: the whole row is the control. The shell gives a
 * row with `onRowClick` its own tab stop and its own Enter/Space handler, so a
 * button here would be a second tab stop on every row doing what the first
 * already does.
 */
export function expandColumn<TData extends RowData>(
  t: (key: string) => string,
) {
  const helper = createAppColumnHelper<TData>();
  return helper.display({
    id: 'expand',
    size: 36,
    minSize: 36,
    maxSize: 36,
    header: () => <span className="sr-only">{t('expand')}</span>,
    cell: ({ row }) => (
      <ChevronRightIcon
        aria-hidden="true"
        className={cn(
          'size-4 text-fg-subtle transition-transform',
          row.getIsExpanded() && 'rotate-90',
        )}
      />
    ),
  });
}
