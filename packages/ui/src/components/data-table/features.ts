import {
  type Column,
  type ColumnDef,
  columnFilteringFeature,
  columnSizingFeature,
  columnVisibilityFeature,
  createColumnHelper,
  globalFilteringFeature,
  metaHelper,
  type RowData,
  rowPaginationFeature,
  rowSelectionFeature,
  rowSortingFeature,
  tableFeatures,
} from '@tanstack/react-table';
import type { IconComponent } from '../icon/icon';
import type { TagTone } from '../tag/tag';

/**
 * One value a filterable column can take: a level, an environment, a release.
 *
 * The `tone` is the same scale `Tag` speaks, because the option list in a
 * filter panel previews exactly what the column will show once the filter
 * applies -- an option painted one red and a cell painted another would read
 * as two different errors.
 */
export interface FilterOption {
  value: string;
  label: string;
  /** Paints the option the way the column paints the value. */
  tone?: TagTone;
  /** What the label falls short of saying: "CI, Docker, releases". */
  hint?: string;
  /**
   * How many rows carry it, computed by the server across the whole result --
   * not by this table, which only ever holds one page and would lie.
   */
  count?: number;
}

/**
 * How a column filters, declared by the shape of its data.
 *
 * This is the decision the column header menu reads: an enum column offers its
 * options with checkboxes, a numeric column offers bounds, a text column
 * offers a contains-match. The column knows its own type; nobody configures
 * the panel separately, so the panel cannot disagree with the data.
 */
export type ColumnFilterSpec =
  | {
      variant: 'options';
      /** The values, when they are known up front: severities, states. */
      options?: readonly FilterOption[];
      /** Or fetched when the panel opens: releases, users. Shows a skeleton. */
      loadOptions?: () => Promise<FilterOption[]>;
      /**
       * Whether more than one value can hold at once. Severities can
       * (`error,fatal`); a date preset cannot. Defaults to true.
       */
      multiple?: boolean;
    }
  | {
      variant: 'text';
      placeholder?: string;
    }
  | {
      variant: 'range';
      min?: number;
      max?: number;
      /** Drawn after the bounds: "ms", "events". */
      unit?: string;
    };

/**
 * What a column says about itself beyond its cells.
 *
 * The same declaration feeds three consumers: the header menu (sort wording,
 * filter panel), the columns menu (label when the header is hidden), and the
 * query bar (which fields can be typed as `key:value`). One source, so the
 * three never disagree about what a column is called or how it filters.
 */
export interface DataTableColumnMeta {
  /** What menus call the column. Falls back to a string header. */
  label?: string;
  /** The icon the query bar shows beside the field's suggestions. */
  icon?: IconComponent;
  /** How it filters. Absent: the column does not filter. */
  filter?: ColumnFilterSpec;
  /** `end` for figures, which right-align so magnitudes line up. */
  align?: 'start' | 'end';
  /**
   * A fixed width in pixels. Left unset the column shares what remains,
   * which is right for exactly one column per table -- the title. Everything
   * measurable (a level, a count, a date) states its width, so the flexible
   * column is the one that absorbs the viewport.
   *
   * A column with a width can be dragged wider or narrower by the reader;
   * the flexible one cannot, since it is whatever the others leave.
   */
  width?: number;
  /** Tabular mono figures: counts, durations. */
  numeric?: boolean;
  /**
   * Optional when space is short: while the table would be wider than its
   * card, columns with a `drop` step aside, lowest first, instead of
   * sliding under the pinned actions column. Absent: always drawn.
   */
  drop?: number;
  /**
   * The sort menu's wording, said in the data's own terms. "Most events
   * first" reads; "Descending" makes the reader translate. Defaults to
   * A→Z / Z→A wording for text and high/low wording for numeric columns.
   */
  sortLabels?: { asc: string; desc: string };
}

/**
 * The features every Rustrak table registers, and no others.
 *
 * There are no client row models here, which is a statement rather than an
 * omission: the server filters, sorts and paginates, and this table renders
 * what it was handed. Registering `createSortedRowModel` would ship dead code
 * and, worse, would make the table quietly re-sort a page the server already
 * sorted.
 */
export const dataTableFeatures = tableFeatures({
  rowSortingFeature,
  columnFilteringFeature,
  globalFilteringFeature,
  rowPaginationFeature,
  columnVisibilityFeature,
  columnSizingFeature,
  rowSelectionFeature,
  columnMeta: metaHelper<DataTableColumnMeta>(),
});

export type DataTableFeatures = typeof dataTableFeatures;

/** A column of a Rustrak table, with the features and meta above baked in. */
export type DataTableColumnDef<TData extends RowData> = ColumnDef<
  DataTableFeatures,
  TData,
  // Columns of one table hold different value types by construction.
  any
>;

/** The column helper, pre-bound so consumers never repeat the generics. */
export function createDataTableColumnHelper<TData extends RowData>() {
  return createColumnHelper<DataTableFeatures, TData>();
}

/** What menus call a column: its meta label, or its header when it is text. */
export function columnLabel<TData extends RowData>(
  column: Column<DataTableFeatures, TData, unknown>,
): string {
  const { meta, header } = column.columnDef;
  if (meta?.label) return meta.label;
  return typeof header === 'string' ? header : column.id;
}
