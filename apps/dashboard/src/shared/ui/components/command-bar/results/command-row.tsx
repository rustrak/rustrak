import type { ReactNode } from 'react';
import { CommandItem } from '../primitives/command';

/** The icon's ink, raised with the row like a menu item's. */
export const ROW_ICON =
  'size-icon-lg text-fg-ghost group-data-[selected=true]/command-item:text-fg-subtle';

/**
 * One line per row, the anatomy of a menu item: icon, label, what it is, and
 * where it lives on the far edge.
 */
export function CommandRow({
  value,
  keywords,
  label,
  description,
  media,
  badge,
  onSelect,
  onHover,
  trailing,
}: {
  /** What the matcher scores against; the label alone is often too thin. */
  value: string;
  keywords?: string[];
  label: string;
  description: string;
  media: ReactNode;
  /** Where the row lives, when the list is mixing several places. */
  badge?: ReactNode;
  onSelect: () => void;
  onHover: () => void;
  trailing?: ReactNode;
}) {
  return (
    <CommandItem
      value={value}
      keywords={keywords}
      onSelect={onSelect}
      // cmdk drives selection from the keyboard only. Without this the pointer
      // moves over a row and nothing happens, which reads as a dead list.
      onPointerMove={onHover}
      className="h-10 gap-3 rounded-lg px-3"
    >
      <span className="flex size-5 shrink-0 items-center justify-center">
        {media}
      </span>
      {/* Takes the slack itself, so the trailing edge lines up on every row
          whatever the label's length. */}
      <span className="flex min-w-0 flex-1 items-baseline gap-2.5">
        <span className="min-w-0 truncate">{label}</span>
        {/* Gives way first, so a long label keeps the row's width. */}
        <span className="min-w-0 shrink-[999] truncate font-normal text-fg-meta text-hint">
          {description}
        </span>
      </span>
      {badge || trailing ? (
        <span className="ms-auto flex shrink-0 items-center gap-3 text-fg-subtle text-hint group-data-[selected=true]/command-item:text-fg">
          {badge}
          {trailing}
        </span>
      ) : null}
    </CommandItem>
  );
}
