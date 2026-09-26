import type { Column, Header, RowData } from '@tanstack/react-table';
import { flexRender } from '@tanstack/react-table';
import { type ReactNode, useState } from 'react';
import { focusRingInset } from '../../lib/focus';
import {
  chevronFlip,
  interactiveTransition,
  pressScaleTrigger,
} from '../../lib/motion';
import { tv } from '../../lib/tv';
import {
  ChevronDownIcon,
  CloseIcon,
  FilterIcon,
  ResolveIcon,
} from '../icon/icon-catalog';
import { Popover } from '../popover/popover';
import type { ColumnFilterSpec, DataTableFeatures } from './features';
import {
  OptionsFilterPanel,
  RangeFilterPanel,
  TextFilterPanel,
} from './filter-panel';

/**
 * A column header that is also the column's control -- the GitHub pattern.
 *
 * Clicking the header does not sort. That convention silently discards work:
 * with a filter panel behind the same click, "sort by this" and "ask about
 * this" cannot share a gesture, and of the two, sorting is the one cheap
 * enough to deserve no panel... but also cheap enough to cost nothing inside
 * one. So the click opens the panel, and the panel's first two rows are the
 * two sorts, worded in the column's own terms. One gesture, everything the
 * column can do, and the panel closes on the choice.
 *
 * What the resting header shows is state, not controls: the lime arrow when
 * it sorts, the lime funnel when it filters. The chevron only surfaces on
 * hover, because fifty resting chevrons would turn the header row into a
 * ribbed texture.
 */
const columnHeader = tv({
  slots: {
    trigger: [
      'group/header flex h-full w-full min-w-0 items-center gap-1.5',
      'font-mono text-column text-fg-meta uppercase',
      'select-none',
      interactiveTransition,
      // Base UI swallows `:active` on anything that opens a popup: it opens on
      // pointer-down and hands capture to the panel. Without this the header
      // gives no feedback at all until the panel appears.
      pressScaleTrigger,
      'hover:text-fg-subtle',
      'data-popup-open:text-fg-subtle',
      focusRingInset,
      'data-[active=true]:text-fg',
      // End-aligned columns keep their reading order -- label, then state --
      // and simply sit against the figures' edge.
      'data-[align=end]:justify-end',
    ],
    label: 'truncate',
    /* State indicators keep the brand colour: they report the query, and the
       query is what the page is about. */
    arrow: 'shrink-0 font-medium text-fg-brand normal-case tracking-normal',
    funnel: 'shrink-0 text-fg-brand',
    chevron: [
      'shrink-0 text-fg-ghost',
      // Surfaces on hover, stays while the panel is open, and flips with it.
      'opacity-0 transition-[opacity,rotate] duration-instant ease-standard',
      'group-hover/header:opacity-100 group-focus-visible/header:opacity-100',
      'group-data-popup-open/header:opacity-100',
      chevronFlip,
    ],
    // A rule between each pair of sections present, whichever those are.
    sections: 'flex flex-col divide-y divide-border',
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

const styles = columnHeader();

/** The wording a sort offers when the column has not chosen its own. */
function sortWording(header: SortableHeader): { asc: string; desc: string } {
  const meta = header.column.columnDef.meta;
  if (meta?.sortLabels) return meta.sortLabels;
  if (meta?.numeric) return { asc: 'Lowest first', desc: 'Highest first' };
  return { asc: 'A to Z', desc: 'Z to A' };
}

// The one alias this file needs; the generics never vary inside it.
type SortableHeader = Header<DataTableFeatures, any, any>;

export interface DataTableColumnHeaderProps<TData extends RowData> {
  header: Header<DataTableFeatures, TData, unknown>;
}

export function DataTableColumnHeader<TData extends RowData>({
  header,
}: DataTableColumnHeaderProps<TData>) {
  const { column } = header;
  const meta = column.columnDef.meta;
  const canSort = column.getCanSort();
  const canHide = column.getCanHide();
  const filterSpec = meta?.filter;
  const [open, setOpen] = useState(false);
  const close = () => setOpen(false);

  const title = flexRender(column.columnDef.header, header.getContext());
  const sorted = column.getIsSorted();
  const filtered = column.getIsFiltered();

  if (!canSort && !filterSpec && !canHide) {
    return (
      <span
        data-align={meta?.align}
        className={styles.trigger({ className: 'cursor-default' })}
      >
        <span className={styles.label()}>{title}</span>
      </span>
    );
  }

  return (
    <Popover
      title={typeof title === 'string' ? title : (meta?.label ?? '')}
      align={meta?.align === 'end' ? 'end' : 'start'}
      popupClassName="w-60"
      open={open}
      onOpenChange={setOpen}
      /* Stays inline: `Popover` hands this to Base UI's `render` prop, which
         merges the open handler, the ref and `data-popup-open` into whatever
         element it is given. A component here would receive all of that as
         props it does not forward, leaving a button that never opens. */
      trigger={
        <button
          type="button"
          data-active={sorted !== false || filtered}
          data-align={meta?.align}
          className={styles.trigger()}
        >
          <TriggerContent title={title} sorted={sorted} filtered={filtered} />
        </button>
      }
    >
      <div className={styles.sections()}>
        {canSort ? (
          <SortSection header={header} sorted={sorted} onDone={close} />
        ) : null}
        {filterSpec ? (
          <FilterSection column={column} spec={filterSpec} />
        ) : null}
        {filtered || canHide ? (
          <ActionsSection
            column={column}
            canClearFilter={filtered}
            canHide={canHide}
          />
        ) : null}
      </div>
    </Popover>
  );
}

/**
 * What the header says about itself: its label, its sort, its filter.
 *
 * Only the children -- the `<button>` around them stays inline in the trigger,
 * where Base UI's `render` prop needs a real element to merge into.
 */
function TriggerContent({
  title,
  sorted,
  filtered,
}: {
  title: ReactNode;
  sorted: false | 'asc' | 'desc';
  filtered: boolean;
}) {
  return (
    <>
      <span className={styles.label()}>{title}</span>
      {sorted ? (
        <span aria-hidden="true" className={styles.arrow()}>
          {sorted === 'desc' ? '↓' : '↑'}
        </span>
      ) : null}
      {filtered ? (
        <FilterIcon size="sm" aria-hidden="true" className={styles.funnel()} />
      ) : null}
      <ChevronDownIcon
        size="sm"
        aria-hidden="true"
        className={styles.chevron()}
      />
    </>
  );
}

/** The two directions, worded for what the column actually holds. */
function SortSection<TData extends RowData>({
  header,
  sorted,
  onDone,
}: {
  header: Header<DataTableFeatures, TData, unknown>;
  sorted: false | 'asc' | 'desc';
  onDone: () => void;
}) {
  const { column } = header;
  const wording = sortWording(header);

  return (
    <div className={styles.section()}>
      {(['asc', 'desc'] as const).map((direction) => (
        <button
          key={direction}
          type="button"
          aria-pressed={sorted === direction}
          className={styles.item()}
          onClick={() => {
            // Choosing the standing sort again clears it: the header is a
            // question, and asking the same question twice withdraws it.
            if (sorted === direction) column.clearSorting();
            else column.toggleSorting(direction === 'desc');
            onDone();
          }}
        >
          <span aria-hidden="true" className="w-3 shrink-0 text-fg-ghost">
            {direction === 'desc' ? '↓' : '↑'}
          </span>
          <span className="min-w-0 flex-1 truncate text-start">
            {wording[direction]}
          </span>
          {sorted === direction ? (
            <ResolveIcon
              size="sm"
              aria-hidden="true"
              className={styles.check()}
            />
          ) : null}
        </button>
      ))}
    </div>
  );
}

/**
 * The panel for a column's filter variant.
 *
 * Keyed rather than chained so a variant added to `ColumnFilterSpec` is a type
 * error here until it has a panel, instead of a column whose filter silently
 * renders nothing.
 */
const FILTER_PANELS = {
  options: OptionsFilterPanel,
  text: TextFilterPanel,
  range: RangeFilterPanel,
} satisfies Record<ColumnFilterSpec['variant'], unknown>;

function FilterSection<TData extends RowData>({
  column,
  spec,
}: {
  column: Column<DataTableFeatures, TData, unknown>;
  spec: ColumnFilterSpec;
}) {
  const Panel = FILTER_PANELS[spec.variant];
  return <Panel column={column} />;
}

/** What can be undone from here: the filter, and the column itself. */
function ActionsSection<TData extends RowData>({
  column,
  canClearFilter,
  canHide,
}: {
  column: Column<DataTableFeatures, TData, unknown>;
  canClearFilter: boolean;
  canHide: boolean;
}) {
  return (
    <div className={styles.section()}>
      {canClearFilter ? (
        <button
          type="button"
          className={styles.item()}
          onClick={() => column.setFilterValue(undefined)}
        >
          <CloseIcon
            size="sm"
            aria-hidden="true"
            className="shrink-0 text-fg-ghost"
          />
          <span className="min-w-0 flex-1 truncate text-start">
            Clear filter
          </span>
        </button>
      ) : null}
      {canHide ? (
        <button
          type="button"
          className={styles.item()}
          onClick={() => column.toggleVisibility(false)}
        >
          <span className="w-3 shrink-0" aria-hidden="true" />
          <span className="min-w-0 flex-1 truncate text-start">
            Hide column
          </span>
        </button>
      ) : null}
    </div>
  );
}

DataTableColumnHeader.displayName = 'DataTableColumnHeader';
