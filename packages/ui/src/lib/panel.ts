import { popTransition } from './motion';

/**
 * The floating panel every popup is drawn as: a menu, a select's list, a
 * popover. To the person looking at it each is "a panel out of that button",
 * and two shades of the same panel read as a bug, so the surface lives here
 * once. Layout (width, padding, scrolling) stays with each component.
 */
export const panelSurface = [
  'rounded-2xl border border-border bg-surface-floating shadow-overlay',
  popTransition,
];

/** A row in a panel's list: a menu item, a select option. */
export const panelItem = [
  'flex h-menu-item cursor-default items-center gap-2.5 rounded-md',
  'text-control text-fg-muted outline-none select-none',
  'data-highlighted:bg-surface-selected data-highlighted:text-fg',
  'data-disabled:text-fg-disabled',
];

/** How far a panel opens from the control that opened it, in px. */
export const PANEL_OFFSET = 6;
