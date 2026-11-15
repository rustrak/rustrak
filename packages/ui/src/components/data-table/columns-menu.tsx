import type { RowData } from '@tanstack/react-table';
import { tv } from '../../lib/tv';
import { Button } from '../button/button';
import { ColumnsIcon, ResolveIcon } from '../icon/icon-catalog';
import { Popover } from '../popover/popover';
import { columnLabel } from './features';
import { useDataTableLabels } from './labels';
import type { DataTableInstance } from './use-data-table';

/**
 * The way back for a hidden column.
 *
 * Hiding happens in the column's own header menu, but a hidden column has no
 * header left to bring it back with -- so the toolbar carries this one
 * button naming every column there is. It is the inverse that makes hiding
 * safe to offer at all.
 */
const columnsMenu = tv({
  slots: {
    section: 'flex flex-col p-1.25',
    item: [
      'flex h-menu-item shrink-0 cursor-default items-center gap-2.5',
      'rounded-sm px-2.5 text-control text-fg-muted outline-none select-none',
      'transition-none',
      'hover:bg-surface-selected hover:text-fg',
      'focus-visible:bg-surface-selected focus-visible:text-fg',
      'aria-pressed:text-fg',
    ],
    check: 'ms-auto shrink-0 text-fg-brand',
  },
});

const styles = columnsMenu();

export interface DataTableColumnsButtonProps<TData extends RowData> {
  table: DataTableInstance<TData>;
}

export function DataTableColumnsButton<TData extends RowData>({
  table,
}: DataTableColumnsButtonProps<TData>) {
  const labels = useDataTableLabels();
  const columns = table
    .getAllLeafColumns()
    .filter((column) => column.getCanHide());

  return (
    <Popover
      title={labels.columns}
      align="end"
      trigger={
        <Button
          variant="secondary"
          icon={ColumnsIcon}
          aria-label={labels.chooseColumns}
          // A phone draws the list, not the columns (`DataTable`), so there
          // is nothing here for it to choose.
          className="max-md:hidden"
        />
      }
    >
      <div className={styles.section()}>
        {columns.map((column) => (
          <button
            key={column.id}
            type="button"
            aria-pressed={column.getIsVisible()}
            className={styles.item()}
            onClick={() => column.toggleVisibility()}
          >
            <span className="min-w-0 flex-1 truncate text-start">
              {columnLabel(column)}
            </span>
            {column.getIsVisible() ? (
              <ResolveIcon
                size="sm"
                aria-hidden="true"
                className={styles.check()}
              />
            ) : null}
          </button>
        ))}
      </div>
    </Popover>
  );
}

DataTableColumnsButton.displayName = 'DataTableColumnsButton';
