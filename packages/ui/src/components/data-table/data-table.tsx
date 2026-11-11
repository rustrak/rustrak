import type { Cell, Header, Row, RowData } from '@tanstack/react-table';
import { flexRender } from '@tanstack/react-table';
import {
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent,
  type ReactNode,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';
import { interactiveTransition, swapAnimation } from '../../lib/motion';
import { tv } from '../../lib/tv';
import { Button } from '../button/button';
import { EmptyIcon } from '../icon/icon-catalog';
import { Text } from '../text/text';
import { DataTableColumnHeader } from './column-header';
import type { DataTableFeatures } from './features';
import { droppedToFit } from './fit';
import { useDataTableLabels } from './labels';
import { DataTablePagination } from './pagination';
import type { DataTableInstance } from './use-data-table';

/**
 * The rendering half of the table; `useDataTable` is the state half.
 *
 * It is one card: rows, their header, and the footer that places you in the
 * result, drawn to the issue-list blueprint -- 38 px header, 48 px rows, the
 * panel surface inside a subtle border. What it renders is decided entirely
 * by the column definitions and the instance's state; the component adds no
 * options of its own beyond the three slots the columns cannot express: what
 * an empty result says, what a loading one shows, and what the header offers
 * once rows are selected.
 */
const dataTable = tv({
  slots: {
    card: [
      'flex min-h-0 flex-col overflow-hidden',
      'rounded-xl border border-border-subtle bg-panel',
    ],
    scroller: 'min-h-0 flex-1 overflow-auto',
    // Fixed layout: the widths in the colgroup are the widths, so dragging
    // one column moves only the flexible one, never the rest.
    table: 'w-full table-fixed border-separate border-spacing-0',
    /*
     * Sticky needs everything else to be true at once: the `th` is the
     * sticking element (a sticky `thead` breaks in Safari), it needs its own
     * opaque background or rows show through it mid-scroll, and the head's
     * rule must be a border on the `th` itself -- with `border-collapse` the
     * border would belong to the table and stay behind while the cell floats.
     */
    th: [
      // Above the row-action overlays, which are z-10 later in tree order.
      'group/th sticky top-0 z-20 h-row-head bg-surface',
      'border-border-subtle border-b px-2',
      'text-start first:pl-4 last:pr-4',
    ],
    td: [
      'h-row border-border-divider border-b px-2',
      'text-body text-fg-secondary',
      'first:pl-4 last:pr-4',
      'data-[align=end]:text-end',
      'data-[numeric=true]:font-mono data-[numeric=true]:text-fg',
      'data-[numeric=true]:text-numeric data-[numeric=true]:tabular-nums',
    ],
    row: [
      'group/row',
      interactiveTransition,
      'hover:bg-surface-hover',
      'data-[selected=true]:bg-surface-raised',
      'data-[clickable=true]:cursor-pointer',
    ],
    /*
     * The header's second life. With rows selected, the column titles step
     * aside and the same 38 px strip holds what can be done to the selection
     * -- the Gmail pattern, because the alternative is a second toolbar that
     * pushes the whole table down exactly when the reader is mid-gesture.
     * Both directions of the swap enter with `swapAnimation`: the outgoing
     * side unmounts, the incoming one rises 2 px into place, and the row
     * itself never moves.
     */
    bulk: ['flex h-full items-center gap-1.5', swapAnimation],
    headerSwap: ['h-full', swapAnimation],
    bulkCount: 'me-2 font-medium text-control text-fg',
    empty: [
      'flex flex-col items-center justify-center gap-3',
      'px-6 py-16 text-center',
    ],
    skeleton: 'h-3 animate-pulse rounded-xs bg-surface-chip',
    /*
     * The row's actions, held to the right edge however far the table is
     * scrolled sideways: what you can do to a row is never off screen.
     * Sticky cells need an opaque ground, so the row's hover and selection
     * tints are repeated on a layer of the cell's own.
     */
    pinned: [
      'sticky right-0 z-10 bg-panel',
      "before:pointer-events-none before:absolute before:inset-0 before:content-['']",
      'group-hover/row:before:bg-surface-hover',
      'group-data-[selected=true]/row:before:bg-surface-raised',
      '[&>*]:relative',
    ],
    pinnedHead: 'right-0 z-30',
    /*
     * The column's right edge, made grabbable. Invisible until the header is
     * hovered, a hairline then, brand while dragging: the reader finds it
     * where every spreadsheet keeps it, and it never decorates a resting
     * header.
     */
    resizer: [
      'absolute top-0 -right-1 z-10 flex h-full w-2 cursor-col-resize',
      'touch-none select-none justify-center',
      'after:h-full after:w-px after:bg-transparent',
      'group-hover/th:after:bg-border-strong',
      'hover:after:bg-fg-ghost data-[resizing=true]:after:bg-border-brand',
      interactiveTransition,
    ],
  },
});

/** The flexible column never gets narrower than this while others grow. */
const FLEX_MIN_WIDTH = 320;

/** No column is dragged narrower than this. */
const MIN_WIDTH = 48;

const styles = dataTable();

export interface DataTableEmptyProps {
  title?: string;
  description?: string;
}

export interface DataTableProps<TData extends RowData> {
  table: DataTableInstance<TData>;
  /**
   * The page is being fetched. With rows on screen they stay, dimmed --
   * yanking a list someone is reading to show a shimmer where it was is the
   * jumpiest thing a refetch can do. Skeletons only stand in when there is
   * nothing yet.
   */
  loading?: boolean;
  /** What an empty result says. The clear-filters offer appears on its own. */
  empty?: DataTableEmptyProps;
  /**
   * What the header offers while rows are selected: resolve, mute, delete.
   * Rendered beside the built-in count and Clear; read the selection off the
   * table instance. Without it the header never changes.
   */
  bulkActions?: ReactNode;
  /** Opens the row. Also offered to the keyboard as Enter on the row. */
  onRowClick?: (row: Row<DataTableFeatures, TData>) => void;
  className?: string;
}

function cellAttributes<TData extends RowData>(
  cell: Cell<DataTableFeatures, TData, unknown>,
) {
  const meta = cell.column.columnDef.meta;
  return {
    'data-align': meta?.align,
    'data-numeric': meta?.numeric || undefined,
  };
}

export function DataTable<TData extends RowData>({
  table,
  loading = false,
  empty,
  bulkActions,
  onRowClick,
  className,
}: DataTableProps<TData>) {
  const labels = useDataTableLabels();
  const rows = table.getRowModel().rows;
  const visible = table.getVisibleLeafColumns();
  const sizing = table.state.columnSizing;
  const sizedWidth = (column: (typeof columns)[number]) =>
    sizing[column.id] ?? column.columnDef.meta?.width;
  // Once the reader has sized every column, the actions column is the one
  // that takes up the slack -- so the menu stays against the right edge.
  const anyFlexible = visible.some(
    (column) =>
      column.id !== 'select' &&
      column.id !== 'actions' &&
      sizedWidth(column) === undefined,
  );
  const widthOf = (column: (typeof visible)[number]) =>
    column.id === 'select'
      ? 40
      : column.id === 'actions'
        ? anyFlexible
          ? 48
          : undefined
        : sizedWidth(column);
  const drawnWidth = (column: (typeof visible)[number]) =>
    widthOf(column) ?? (column.id === 'actions' ? 48 : FLEX_MIN_WIDTH);
  // The card's width, measured: optional columns (`meta.drop`) step aside
  // when the rest would not fit, before the table scrolls under the pinned
  // actions column.
  const scroller = useRef<HTMLDivElement>(null);
  const [available, setAvailable] = useState<number>();
  useLayoutEffect(() => {
    const node = scroller.current;
    if (!node) return;
    const observer = new ResizeObserver(() => setAvailable(node.clientWidth));
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  const dropped = droppedToFit(
    visible.map((column) => ({
      id: column.id,
      width: drawnWidth(column),
      drop: column.columnDef.meta?.drop,
    })),
    available,
  );
  const columns = visible.filter((column) => !dropped.has(column.id));
  const shown = <T extends { column: { id: string } }>(items: T[]) =>
    items.filter((item) => !dropped.has(item.column.id));
  // Wide enough for every sized column and a readable flexible one; past
  // that the card scrolls sideways instead of crushing the title.
  const minWidth = columns.reduce((sum, column) => sum + drawnWidth(column), 0);
  const filtered =
    table.state.columnFilters.length > 0 || Boolean(table.state.globalFilter);
  const selectedCount = Object.keys(table.state.rowSelection).length;
  const bulkMode = bulkActions != null && selectedCount > 0;

  /*
   * A click that lands on a control inside the row -- the checkbox, an
   * action, a link a cell rendered -- belongs to that control. Only the inert
   * remainder of the row opens it.
   */
  function handleRowClick(
    row: Row<DataTableFeatures, TData>,
    event: MouseEvent,
  ) {
    if (!onRowClick) return;
    const target = event.target as HTMLElement;
    // A portalled popup (the row's menu) bubbles here too, from outside the row.
    if (!(event.currentTarget as HTMLElement).contains(target)) return;
    if (target.closest('button, a, input, label, [role="checkbox"]')) return;
    onRowClick(row);
  }

  /*
   * The list is worked from the keyboard the way a mail client is: arrows
   * or j/k walk the rows, x ticks the one you are on, Enter opens it.
   * Focus moves between the rows' own tab stops, so the browser's focus ring
   * is the cursor and screen readers follow it.
   */
  function handleRowKey(
    event: KeyboardEvent<HTMLTableRowElement>,
    row: Row<DataTableFeatures, TData>,
  ) {
    // Cmd+K, Ctrl+J, Cmd+ArrowDown belong to the browser or the app, not to
    // the row: the plain keys are the row's.
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    const step = { ArrowDown: 1, j: 1, ArrowUp: -1, k: -1 }[event.key];
    if (step) {
      event.preventDefault();
      const sibling =
        step > 0
          ? event.currentTarget.nextElementSibling
          : event.currentTarget.previousElementSibling;
      (sibling as HTMLElement | null)?.focus();
      return;
    }
    if (event.key === 'x' && row.getCanSelect()) {
      event.preventDefault();
      row.toggleSelected();
      return;
    }
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onRowClick?.(row);
    }
  }

  return (
    <div className={styles.card({ className })}>
      <div ref={scroller} className={styles.scroller()} aria-busy={loading}>
        <table className={styles.table()} style={{ minWidth }}>
          <colgroup>
            {columns.map((column) => (
              <col key={column.id} style={{ width: widthOf(column) }} />
            ))}
          </colgroup>

          <thead>
            {table.getHeaderGroups().map((headerGroup) => (
              <tr key={headerGroup.id}>
                {bulkMode && headerGroup.headers[0]?.column.id === 'select' ? (
                  <>
                    {/* The box stays: it is how the selection is grown to the
                        page or emptied, and it must not jump. */}
                    <th scope="col" className={styles.th()}>
                      <span className="flex items-center">
                        {flexRender(
                          headerGroup.headers[0].column.columnDef.header,
                          headerGroup.headers[0].getContext(),
                        )}
                      </span>
                    </th>
                    <th
                      scope="col"
                      colSpan={shown(headerGroup.headers).length - 1}
                      className={styles.th()}
                    >
                      <div
                        role="toolbar"
                        aria-label={labels.selectionToolbar}
                        className={styles.bulk()}
                      >
                        <span aria-live="polite" className={styles.bulkCount()}>
                          {labels.selected(selectedCount)}
                        </span>
                        {bulkActions}
                        <Button
                          variant="ghost"
                          size="xs"
                          className="ms-auto"
                          onClick={() => table.toggleAllRowsSelected(false)}
                        >
                          {labels.clearSelection}
                        </Button>
                      </div>
                    </th>
                  </>
                ) : (
                  shown(headerGroup.headers).map((header) => (
                    <HeaderCell
                      key={header.id}
                      header={header}
                      width={widthOf(header.column)}
                    />
                  ))
                )}
              </tr>
            ))}
          </thead>

          <tbody className={loading && rows.length ? 'opacity-60' : undefined}>
            {rows.map((row) => (
              <tr
                key={row.id}
                data-selected={row.getIsSelected() || undefined}
                data-clickable={onRowClick ? true : undefined}
                // A `role` override would break the row/cell relationship a
                // table needs; the row stays a row and only gains a tab stop.
                tabIndex={onRowClick ? 0 : undefined}
                onClick={(event) => handleRowClick(row, event)}
                onKeyDown={(event) => {
                  if (!onRowClick) return;
                  if (event.target !== event.currentTarget) return;
                  handleRowKey(event, row);
                }}
                className={styles.row()}
              >
                {shown(row.getVisibleCells()).map((cell) => (
                  <td
                    key={cell.id}
                    {...cellAttributes(cell)}
                    className={styles.td({
                      className:
                        cell.column.id === 'actions'
                          ? styles.pinned()
                          : undefined,
                    })}
                  >
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </td>
                ))}
              </tr>
            ))}

            {loading && rows.length === 0
              ? Array.from({ length: 8 }, (_, line) => (
                  // Skeleton rows have no identity beyond their position.
                  <tr key={line} className={styles.row()}>
                    {columns.map((column) => (
                      <td key={column.id} className={styles.td()}>
                        <span
                          aria-hidden="true"
                          className={styles.skeleton({
                            className: column.columnDef.meta?.width
                              ? 'block w-3/5'
                              : 'block w-4/5',
                          })}
                        />
                      </td>
                    ))}
                  </tr>
                ))
              : null}
          </tbody>
        </table>

        {!loading && rows.length === 0 ? (
          <div className={styles.empty()}>
            <EmptyIcon
              size="2xl"
              aria-hidden="true"
              className="text-fg-ghost"
            />
            <div className="flex flex-col gap-1">
              <Text variant="card-title">
                {empty?.title ?? labels.noResults}
              </Text>
              <Text variant="meta" tone="subtle">
                {empty?.description ??
                  (filtered ? labels.noMatches : labels.nothingYet)}
              </Text>
            </div>
            {filtered ? (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  table.setColumnFilters([]);
                  table.setGlobalFilter('');
                }}
              >
                {labels.clearFilters}
              </Button>
            ) : null}
          </div>
        ) : null}
      </div>

      <DataTablePagination table={table} />
    </div>
  );
}

DataTable.displayName = 'DataTable';

/**
 * One column heading.
 *
 * `aria-sort` is on the `th` rather than on the button inside it because the
 * sort state describes the column, and a screen reader reads it while moving
 * between columns, not only when it lands on the control.
 */
function HeaderCell<TData extends RowData>({
  header,
  width,
}: {
  header: Header<DataTableFeatures, TData, unknown>;
  /** The width the reader set or the design states; none when flexible. */
  width: number | undefined;
}) {
  const sorted = header.column.getIsSorted();
  const ariaSort =
    sorted === 'asc'
      ? 'ascending'
      : sorted === 'desc'
        ? 'descending'
        : undefined;

  return (
    <th
      scope="col"
      colSpan={header.colSpan}
      aria-sort={ariaSort}
      className={styles.th({
        className:
          header.column.id === 'actions' ? styles.pinnedHead() : undefined,
      })}
    >
      <HeaderContent header={header} />
      {/* The two injected columns have one width by design. */}
      {header.column.id !== 'select' && header.column.id !== 'actions' ? (
        <Resizer header={header} width={width} />
      ) : null}
    </th>
  );
}

/** The heading itself: the select box renders bare, every other column swaps. */
function HeaderContent<TData extends RowData>({
  header,
}: {
  header: Header<DataTableFeatures, TData, unknown>;
}) {
  if (header.isPlaceholder) return null;

  if (header.column.id === 'select') {
    return (
      <span className="flex items-center">
        {flexRender(header.column.columnDef.header, header.getContext())}
      </span>
    );
  }

  return (
    /* Re-enters the same way the bulk strip arrived, so the swap reads as one
       exchange, not two events. */
    <div className={styles.headerSwap()}>
      <DataTableColumnHeader header={header} />
    </div>
  );
}

/**
 * The handle on a column's right edge. Drag to size it, double-click to put
 * it back to the width it was designed at -- or back to flexible, for the
 * column that had none. The arrow keys size it too, so the gesture is not
 * pointer-only.
 *
 * Every drag starts from the width the column is drawn at, measured, not
 * from a number in state: the flexible column has no number until the
 * reader gives it one, and it must not jump to some default when grabbed.
 */
function Resizer<TData extends RowData>({
  header,
  width,
}: {
  header: Header<DataTableFeatures, TData, unknown>;
  width: number | undefined;
}) {
  const labels = useDataTableLabels();
  const [dragging, setDragging] = useState(false);
  const handle = useRef<HTMLDivElement>(null);
  const endDrag = useRef<(() => void) | null>(null);
  useEffect(() => () => endDrag.current?.(), []);
  // The flexible column has no width in state; what it is drawn at is still
  // its value, and a separator a screen reader can move must say one.
  const [drawn, setDrawn] = useState(width ?? 0);
  useLayoutEffect(() => {
    const cell = handle.current?.parentElement;
    if (cell) setDrawn(Math.round(cell.getBoundingClientRect().width));
  }, [width]);
  const { column } = header;
  const { table } = header.getContext();

  const resize = (next: number) =>
    table.setColumnSizing((previous) => ({
      ...previous,
      [column.id]: Math.max(MIN_WIDTH, Math.round(next)),
    }));

  const drawnWidth = (handle: HTMLElement) =>
    handle.parentElement?.getBoundingClientRect().width ?? width ?? 0;

  function startDrag(event: PointerEvent<HTMLDivElement>) {
    // The primary button only: a right-click opens a menu, it does not drag.
    if (event.button !== 0) return;
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = drawnWidth(event.currentTarget);

    const move = (moved: globalThis.PointerEvent) =>
      resize(startWidth + moved.clientX - startX);
    // A drag ends on release, on a cancelled touch or pen, or when the handle
    // goes away mid-drag; any of them, or the column keeps following the
    // pointer.
    const end = () => {
      setDragging(false);
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', end);
      window.removeEventListener('pointercancel', end);
      endDrag.current = null;
    };

    setDragging(true);
    endDrag.current = end;
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', end);
    window.addEventListener('pointercancel', end);
  }

  return (
    // biome-ignore lint/a11y/useSemanticElements: a separator that is also a slider is the ARIA pattern for a resizable splitter; there is no element for it.
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={labels.resizeColumn}
      aria-valuenow={width ?? drawn}
      ref={handle}
      tabIndex={0}
      data-resizing={dragging || undefined}
      className={styles.resizer()}
      onPointerDown={startDrag}
      onDoubleClick={() =>
        table.setColumnSizing((previous) => {
          const { [column.id]: _, ...rest } = previous;
          return rest;
        })
      }
      onKeyDown={(event) => {
        const delta = { ArrowLeft: -16, ArrowRight: 16 }[event.key];
        if (!delta) return;
        event.preventDefault();
        resize(drawnWidth(event.currentTarget) + delta);
      }}
    />
  );
}
