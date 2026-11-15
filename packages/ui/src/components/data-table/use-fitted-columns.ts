import type { Column, Header, RowData } from '@tanstack/react-table';
import { useLayoutEffect, useRef, useState } from 'react';
import { useMobileBreakpoint } from '../../lib/use-mobile';
import type { DataTableFeatures } from './features';
import { droppedToFit } from './fit';
import type { DataTableInstance } from './use-data-table';

/** The flexible column never gets narrower than this while others grow. */
const FLEX_MIN_WIDTH = 320;

/**
 * The same on a phone: narrow enough that the title and the figures a list
 * keeps there (`meta.mobile`) fit a 360 px screen, 160 + 104 + 88 at most.
 */
const PHONE_FLEX_MIN_WIDTH = 160;

type FitColumn<TData extends RowData> = Column<
  DataTableFeatures,
  TData,
  unknown
>;

/**
 * What a phone does not draw: the ticks, and every fixed-width column that
 * does not say `mobile`. What is left is a list -- the flexible column, the
 * figures it keeps, and the row's actions.
 */
function onPhone<TData extends RowData>(column: FitColumn<TData>): boolean {
  const meta = column.columnDef.meta;
  return (
    column.id !== 'select' &&
    (column.id === 'actions' || meta?.width === undefined || !!meta.mobile)
  );
}

/**
 * Which columns a table draws and how wide, for the card it is in.
 *
 * Optional columns (`meta.drop`) step aside when the rest would not fit,
 * before the table scrolls sideways under the pinned actions column. On a
 * phone the table is a list first (`onPhone`), and the widths a reader dragged
 * on a desktop do not apply: they are kept for the desktop, not drawn here.
 */
export function useFittedColumns<TData extends RowData>(
  table: DataTableInstance<TData>,
) {
  const { mobile } = useMobileBreakpoint();
  const flexMinWidth = mobile ? PHONE_FLEX_MIN_WIDTH : FLEX_MIN_WIDTH;
  const visible = table
    .getVisibleLeafColumns()
    .filter((column) => !mobile || onPhone(column));
  const sizing = mobile ? {} : table.state.columnSizing;
  const sizedWidth = (column: FitColumn<TData>) =>
    sizing[column.id] ?? column.columnDef.meta?.width;
  // Once the reader has sized every column, the actions column is the one
  // that takes up the slack -- so the menu stays against the right edge.
  const anyFlexible = visible.some(
    (column) =>
      column.id !== 'select' &&
      column.id !== 'actions' &&
      sizedWidth(column) === undefined,
  );
  const widthOf = (column: FitColumn<TData>) => {
    if (column.id === 'select') return 40;
    if (column.id === 'actions') return anyFlexible ? 48 : undefined;
    return sizedWidth(column);
  };
  const drawnWidth = (column: FitColumn<TData>) =>
    widthOf(column) ?? (column.id === 'actions' ? 48 : flexMinWidth);

  const scroller = useRef<HTMLDivElement>(null);
  const [available, setAvailable] = useState<number>();
  useLayoutEffect(() => {
    const node = scroller.current;
    if (!node) return;
    const observer = new ResizeObserver(() => setAvailable(node.clientWidth));
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  // The column under a resize handle never steps aside mid-gesture: its
  // handle holds the focus, and the next key must be able to undo the last.
  const [resizing, setResizing] = useState<string>();
  const dropped = droppedToFit(
    visible.map((column) => ({
      id: column.id,
      width: drawnWidth(column),
      drop: column.id === resizing ? undefined : column.columnDef.meta?.drop,
    })),
    available,
  );
  const drawn = new Set(
    visible.filter((column) => !dropped.has(column.id)).map((c) => c.id),
  );
  const columns = visible.filter((column) => drawn.has(column.id));

  return {
    columns,
    widthOf,
    scroller,
    setResizing,
    /** Only the cells and headers of drawn columns. */
    shown: <T extends { column: { id: string } }>(items: T[]) =>
      items.filter((item) => drawn.has(item.column.id)),
    // A header spans the leaves still drawn under it; a group with none left
    // spans nothing and is not drawn.
    spanOf: (header: Header<DataTableFeatures, TData, unknown>) =>
      header
        .getLeafHeaders()
        .filter(
          (leaf) => leaf.subHeaders.length === 0 && drawn.has(leaf.column.id),
        ).length,
    // Wide enough for every sized column and a readable flexible one; past
    // that the card scrolls sideways instead of crushing the title.
    minWidth: columns.reduce((sum, column) => sum + drawnWidth(column), 0),
  };
}
