import type { Column, RowData } from '@tanstack/react-table';
import { Button } from '../button/button';
import { Menu } from '../menu/menu';
import type { MenuAction } from '../menu/menu-parts';
import { sortWording } from './column-header';
import { columnLabel, type DataTableFeatures } from './features';
import { useDataTableLabels } from './labels';
import type { DataTableInstance } from './use-data-table';

/**
 * The column head on a phone: the list's name, and the sort.
 *
 * A phone draws the list without its figure columns (`DataTable`), and those
 * headers were where sorting lived. So the sort moves here, into one menu that
 * offers every sortable column, drawn or not: "Events, most first", "Last
 * seen, newest first". Choosing the standing sort again clears it, as the
 * column header does.
 */
export function DataTablePhoneHead<TData extends RowData>({
  table,
  columns,
  className,
}: {
  table: DataTableInstance<TData>;
  /** The columns drawn, for the colspan and the list's name. */
  columns: Column<DataTableFeatures, TData, unknown>[];
  className: string;
}) {
  const labels = useDataTableLabels();
  const sortable = table.getAllLeafColumns().filter((c) => c.getCanSort());
  const current = sortable.find((c) => c.getIsSorted());
  const identity = columns.find((c) => c.id !== 'select');

  const actions: MenuAction[] = sortable.flatMap((column) => {
    const wording = sortWording(column.columnDef.meta, labels);
    return (['desc', 'asc'] as const).map((direction) => ({
      id: `${column.id}:${direction}`,
      label: `${columnLabel(column)} · ${wording[direction]}`,
      selected: column.getIsSorted() === direction,
      onSelect: () =>
        column.getIsSorted() === direction
          ? column.clearSorting()
          : column.toggleSorting(direction === 'desc'),
    }));
  });

  return (
    <tr className="md:hidden">
      <th scope="col" colSpan={columns.length} className={className}>
        <span className="flex items-center justify-between gap-2">
          <span className="truncate text-column text-fg-subtle">
            {identity ? columnLabel(identity) : null}
          </span>
          {actions.length > 0 ? (
            <Menu
              trigger={
                <Button variant="ghost" size="xs">
                  {current
                    ? `${current.getIsSorted() === 'desc' ? '↓' : '↑'} ${columnLabel(current)}`
                    : labels.sortBy}
                </Button>
              }
              actions={actions}
              popupClassName="w-72"
            />
          ) : null}
        </span>
      </th>
    </tr>
  );
}
