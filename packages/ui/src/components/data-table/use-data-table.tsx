import {
  type ColumnVisibilityState,
  functionalUpdate,
  type ReactTable,
  type Row,
  type RowData,
  type RowSelectionState,
  type Updater,
  useTable,
} from '@tanstack/react-table';
import { useMemo, useState } from 'react';
import { Button } from '../button/button';
import { Checkbox } from '../checkbox/checkbox';
import { OverflowIcon } from '../icon/icon-catalog';
import { Menu } from '../menu/menu';
import type { MenuAction } from '../menu/menu-parts';
import {
  type DataTableColumnDef,
  type DataTableFeatures,
  dataTableFeatures,
} from './features';
import type { DataTableQuery } from './query';

/** The table instance, with the Rustrak features baked into the type. */
export type DataTableInstance<TData extends RowData> = ReactTable<
  DataTableFeatures,
  TData
>;

export interface UseDataTableOptions<TData extends RowData> {
  /** One page, exactly as the server returned it. */
  data: TData[];
  columns: DataTableColumnDef<TData>[];
  /** How many rows exist across every page. Drives the footer and the count. */
  rowCount: number;
  /** The URL-worthy state. The table proposes changes; the owner applies them. */
  query: DataTableQuery;
  onQueryChange: (
    updater: (previous: DataTableQuery) => DataTableQuery,
  ) => void;
  /**
   * The row's identity for selection, which must survive a refetch: an index
   * would tick whatever row happens to land third after the next poll.
   */
  getRowId?: (row: TData) => string;
  /** Adds the checkbox column and Shift-click range selection. */
  enableSelection?: boolean;
  /**
   * The actions a row offers, as data. Adds a fixed last column holding the
   * one control every row ends with: the ⋯ menu. Actions as `MenuAction[]`
   * rather than JSX, for the same reason `Menu` takes them that way -- the
   * list is data, and data does not drift between rows.
   *
   * Like `columns`, give it a stable identity (module scope or `useCallback`):
   * a new function every render rebuilds the table's column model with it.
   */
  rowMenu?: (row: Row<DataTableFeatures, TData>) => MenuAction[];
}

/**
 * The state half of the table: TanStack Table v9 in fully manual mode.
 *
 * The server filters, sorts and paginates -- this hook registers no client row
 * models, so there is no code path in which the table second-guesses a page
 * the server already shaped. What the hook owns is the *proposal* side:
 * a header click proposes a sort, and the proposal lands in `onQueryChange`
 * as a pure updater for whoever owns the state -- a `useState` in Storybook,
 * the URL in the dashboard.
 *
 * Any change of filters, search or sort rewinds to the first page in the same
 * update. Manual pagination switches TanStack's auto-reset off, so without
 * this line a narrowed search would leave you stranded on page 7 of a
 * result that now has one page -- and it must happen here, atomically, not as
 * an effect in the app that briefly requests page 7 of the new query.
 *
 * Selection and column visibility stay inside the hook: they are gestures and
 * reading preferences, not places, so they do not belong to the URL owner
 * (see `DataTableQuery`).
 */
export function useDataTable<TData extends RowData>({
  data,
  columns,
  rowCount,
  query,
  onQueryChange,
  getRowId,
  enableSelection = false,
  rowMenu,
}: UseDataTableOptions<TData>): DataTableInstance<TData> {
  if (process.env.NODE_ENV !== 'production' && enableSelection && !getRowId) {
    // TanStack falls back to the row's index, which selection can't survive:
    // a refetch or a page change reassigns "row 0" to a different row.
    console.error(
      'useDataTable: enableSelection is true without getRowId. Selection ' +
        'will not survive a refetch or a page change -- pass getRowId.',
    );
  }

  const [rowSelection, setRowSelection] = useState<RowSelectionState>({});
  const [columnVisibility, setColumnVisibility] =
    useState<ColumnVisibilityState>({});

  const allColumns = useMemo(
    () => [
      ...(enableSelection ? [selectionColumn<TData>()] : []),
      ...columns,
      ...(rowMenu ? [menuColumn<TData>(rowMenu)] : []),
    ],
    [enableSelection, columns, rowMenu],
  );

  function applyQuery<TSlice extends 'sorting' | 'filters' | 'search'>(
    slice: TSlice,
    updater: Updater<DataTableQuery[TSlice]>,
  ) {
    onQueryChange((previous) => {
      const next = functionalUpdate(updater, previous[slice]);
      return {
        ...previous,
        // TanStack's global filter can be set to `undefined`; `search` never is.
        [slice]: slice === 'search' && next == null ? '' : next,
        pagination: { ...previous.pagination, pageIndex: 0 },
      };
    });
  }

  return useTable<DataTableFeatures, TData>({
    features: dataTableFeatures,
    columns: allColumns,
    data,
    getRowId,
    rowCount,
    manualSorting: true,
    manualFiltering: true,
    manualPagination: true,
    enableRowSelection: enableSelection,
    state: {
      sorting: query.sorting,
      columnFilters: query.filters,
      globalFilter: query.search,
      pagination: query.pagination,
      rowSelection,
      columnVisibility,
    },
    onSortingChange: (updater) => applyQuery('sorting', updater),
    onColumnFiltersChange: (updater) => applyQuery('filters', updater),
    onGlobalFilterChange: (updater) => applyQuery('search', updater),
    onPaginationChange: (updater) =>
      onQueryChange((previous) => ({
        ...previous,
        pagination: functionalUpdate(updater, previous.pagination),
      })),
    onRowSelectionChange: setRowSelection,
    onColumnVisibilityChange: setColumnVisibility,
  });
}

/**
 * The checkbox column, injected rather than configured: every selectable
 * table in the product gets the identical first column, and identical is the
 * point.
 *
 * The header box reports the page, not the result: "all" means all 50 rows in
 * front of you, because ticking a box must never select the 12,000 rows it
 * stands for. Bulk actions on the whole result are a different, spoken-out
 * gesture.
 */
function selectionColumn<TData extends RowData>(): DataTableColumnDef<TData> {
  return {
    id: 'select',
    enableSorting: false,
    enableHiding: false,
    header: ({ table }) => {
      const all = table.getIsAllPageRowsSelected();
      return (
        <Checkbox
          aria-label="Select all rows on this page"
          checked={all}
          indeterminate={table.getIsSomePageRowsSelected() && !all}
          onCheckedChange={() => table.toggleAllPageRowsSelected()}
        />
      );
    },
    cell: ({ row }) => (
      <Checkbox
        aria-label="Select row"
        checked={row.getIsSelected()}
        disabled={!row.getCanSelect()}
        onCheckedChange={(checked) => row.toggleSelected(checked)}
      />
    ),
  };
}

/**
 * The ⋯ column: always the last one, always visible, one width.
 *
 * The actions used to surface on hover, riding the row's right edge. That
 * pattern went: what only exists under the pointer cannot be discovered by
 * reading, does not exist on a touch screen, and covered the very figures
 * the row was sorted by. A resting ⋯ costs 40 px and answers "what can I do
 * to this row?" before the question is asked.
 */
function menuColumn<TData extends RowData>(
  rowMenu: (row: Row<DataTableFeatures, TData>) => MenuAction[],
): DataTableColumnDef<TData> {
  return {
    id: 'actions',
    enableSorting: false,
    enableHiding: false,
    header: () => <span className="sr-only">Actions</span>,
    cell: ({ row }) => (
      <span className="flex items-center justify-end">
        <Menu
          align="end"
          actions={rowMenu(row)}
          trigger={
            <Button
              variant="ghost"
              size="xs"
              icon={OverflowIcon}
              aria-label="Row actions"
            />
          }
        />
      </span>
    ),
  };
}
