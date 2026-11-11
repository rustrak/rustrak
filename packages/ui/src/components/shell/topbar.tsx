import type { ComponentPropsWithoutRef, ReactNode } from 'react';
import { cn } from '../../lib/cn';
import { focusRing } from '../../lib/focus';
import { interactiveTransition, pressScaleSmall } from '../../lib/motion';
import { tv } from '../../lib/tv';
import { CloseIcon, MenuIcon } from '../icon/icon-catalog';
import { useSidebar } from './sidebar-context';

/**
 * The topbar: the 48 px strip across the top of the sheet. Where you are on
 * the left, what the page is scoped to on the right.
 *
 * It belongs to the sheet, not to the window. Everything that does not change
 * when you change page -- the mark, search, the account -- lives in the
 * sidebar, so what is left up here is only ever about the page underneath it.
 * That is what keeps it quiet: a breadcrumb trail and, at most, an environment
 * and a period.
 */
const topbar = tv({
  base: [
    'flex h-topbar w-full shrink-0 items-center gap-2',
    'border-b border-border-divider ps-5 pe-3 text-fg-subtle',
    'max-md:ps-3',
  ],
});

export interface TopbarProps extends ComponentPropsWithoutRef<'header'> {
  /** What scopes the page: an environment, a period. Pushed right. */
  actions?: ReactNode;
  /**
   * What opens the navigation on a phone, before the trail. Only visible below
   * `md`, which is where the sidebar stops being on screen permanently.
   */
  menu?: ReactNode;
}

export function Topbar({
  actions,
  menu,
  children,
  className,
  ...props
}: TopbarProps) {
  return (
    <header className={topbar({ className })} {...props}>
      {menu}
      <div className="flex min-w-0 flex-1 items-center">{children}</div>
      {actions ? (
        <div className="flex shrink-0 items-center gap-2">{actions}</div>
      ) : null}
    </header>
  );
}

Topbar.displayName = 'Topbar';

/**
 * The button that opens the navigation on a phone.
 *
 * It only exists below `md`. Above that the sidebar is on screen already, and a
 * button that opens what you can see says nothing.
 */
export function TopbarMenuButton({
  openLabel,
  closeLabel,
}: {
  /** "Open navigation", in the reader's language. */
  openLabel: string;
  /** "Close navigation", in the reader's language. */
  closeLabel: string;
}) {
  const { drawerOpen, toggleDrawer } = useSidebar();

  return (
    <button
      type="button"
      onClick={toggleDrawer}
      aria-label={drawerOpen ? closeLabel : openLabel}
      aria-expanded={drawerOpen}
      className={cn(
        'flex size-control-sm shrink-0 items-center justify-center md:hidden',
        'rounded-lg text-fg-subtle hover:bg-surface-hover hover:text-fg',
        interactiveTransition,
        pressScaleSmall,
        focusRing,
      )}
    >
      {drawerOpen ? <CloseIcon size="xl" /> : <MenuIcon size="xl" />}
    </button>
  );
}

TopbarMenuButton.displayName = 'TopbarMenuButton';
