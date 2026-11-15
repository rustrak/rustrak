import type { Column, RowData } from '@tanstack/react-table';
import type { ReactNode } from 'react';
import { useMobileBreakpoint } from '../../lib/use-mobile';
import { Button } from '../button/button';
import { Menu } from '../menu/menu';
import type { MenuAction } from '../menu/menu-parts';
import { columnLabel, type DataTableFeatures, sortWording } from './features';
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
  bulkToolbar,
}: {
  table: DataTableInstance<TData>;
  /** The columns drawn: one heading each. */
  columns: Column<DataTableFeatures, TData, unknown>[];
  className: string;
  /** While rows are selected, the head is the selection's toolbar instead. */
  bulkToolbar?: ReactNode;
}) {
  const labels = useDataTableLabels();
  // Not in the DOM at all above `md`: a second, hidden set of headings would
  // be a second answer to "which column is Events" for anything that asks.
  const { mobile } = useMobileBreakpoint();
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

  if (!mobile) return null;

  if (bulkToolbar) {
    return (
      <tr>
        <th scope="col" colSpan={columns.length} className={className}>
          {bulkToolbar}
        </th>
      </tr>
    );
  }

  const sortMenu =
    actions.length > 0 ? (
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
    ) : null;

  // One heading per drawn column, so a figure a phone keeps (`meta.mobile`)
  // is still named, for the eye and for a screen reader. The list's own
  // heading carries the sort.
  return (
    <tr className="md:hidden">
      {columns.map((column) => (
        <th
          key={column.id}
          scope="col"
          data-align={column.columnDef.meta?.align}
          className={className}
        >
          {column.id === identity?.id ? (
            <span className="flex items-center justify-between gap-2">
              <span className="truncate text-column text-fg-subtle">
                {columnLabel(column)}
              </span>
              {sortMenu}
            </span>
          ) : column.id === 'actions' ? (
            <span className="sr-only">{labels.actions}</span>
          ) : (
            <span className="block truncate text-column text-fg-subtle text-end">
              {columnLabel(column)}
            </span>
          )}
        </th>
      ))}
    </tr>
  );
}
