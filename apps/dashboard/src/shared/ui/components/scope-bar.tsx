import {
  ChevronDownIcon,
  cn,
  focusRing,
  type IconComponent,
  interactiveTransition,
  Menu,
  type MenuAction,
} from '@rustrak/ui';
import type { ReactNode } from 'react';

/**
 * The bar a page scopes itself with, in its header: environment, release,
 * window. Its filters scope every panel below at once, which is what sets
 * them apart from a table's own filters, that live with the table.
 *
 * Separate controls side by side, each its own pill: joined into one bar
 * they read as a single button with dividers. Weight follows meaning: a
 * scope that is not set shows only its name, quietly; one that is set shows
 * its value, bright, with the brand dot.
 */
export function ScopeBar({ children }: { children: ReactNode }) {
  return (
    <div className="flex w-full flex-wrap gap-1.5 sm:inline-flex sm:w-auto">
      {children}
    </div>
  );
}

/** One part of the bar: a menu, its icon and what it is set to. */
export function ScopeSegment({
  icon: Icon,
  label,
  active,
  mono,
  actions,
}: {
  icon: IconComponent;
  label: ReactNode;
  /** A scope that is set; the window is always set, so it never is. */
  active?: boolean;
  mono?: boolean;
  actions: MenuAction[];
}) {
  // Its own box: an open menu puts focus guards beside its trigger, and the
  // box keeps them from taking part in the bar's gap.
  return (
    <div
      className={cn(
        'flex min-w-0 flex-1 rounded-lg border bg-surface sm:max-w-48 sm:flex-none',
        active ? 'border-border-strong' : 'border-border-subtle',
      )}
    >
      <Menu
        align="end"
        actions={actions}
        popupClassName="max-h-80 min-w-48 overflow-y-auto"
        trigger={
          <button
            type="button"
            className={cn(
              'flex h-control-md min-w-0 flex-1 items-center gap-1.5 px-2.5 text-control',
              // The corners are the box's, so the hover fill follows them.
              'rounded-[inherit]',
              'hover:bg-surface-hover hover:text-fg data-popup-open:bg-surface-hover',
              active ? 'font-medium text-fg' : 'text-fg-subtle',
              interactiveTransition,
              focusRing,
            )}
          >
            {active ? (
              <span
                aria-hidden="true"
                className="size-1.5 shrink-0 rounded-pill bg-surface-brand"
              />
            ) : (
              <Icon size="sm" className="shrink-0" />
            )}
            <span className={cn('truncate', mono && 'font-mono')}>{label}</span>
            <ChevronDownIcon
              size="sm"
              className="ms-auto shrink-0 text-fg-ghost"
            />
          </button>
        }
      />
    </div>
  );
}
