import { mergeProps } from '@base-ui/react/merge-props';
import { useRender } from '@base-ui/react/use-render';
import type { ComponentPropsWithoutRef, ReactNode } from 'react';
import { focusRingInset } from '../../lib/focus';
import { tv } from '../../lib/tv';
import type { IconComponent } from '../icon/icon';

/**
 * The bottom tab bar: a phone's navigation, where the sidebar is on a desktop.
 *
 * It sits **under** the content, in the shell's column, not floating over it.
 * A bar laid over the page hides the last row of every list and asks each
 * screen to pad itself for it; a bar that takes its own 56 px leaves the
 * content's box honest, and the last row is the last thing you scroll to.
 *
 * Below `md` only, which `AppShell` decides when it is given one: from `md`
 * up the sidebar has room, and two navigations on one screen is one too many.
 *
 * The home indicator is padded below it (`pb-safe-bottom`), so on an iPhone the
 * targets stay above the swipe area and the bar's colour runs to the edge.
 */
const tabBar = tv({
  slots: {
    root: [
      'grid shrink-0 auto-cols-fr grid-flow-col print:hidden',
      'border-t border-border-divider bg-panel pb-safe-bottom',
      'select-none',
    ],
    item: [
      'group/tab relative flex h-tab-bar min-w-0 flex-col items-center justify-center gap-1',
      'text-fg-subtle outline-none',
      'aria-[current=page]:text-fg',
      // The colour changes at once: a tab is a place, and the place is the
      // page that just changed under it.
      focusRingInset,
    ],
    /*
     * The active tab's pill, behind the icon: Material 3's navigation bar
     * indicator. A tinted capsule says "you are here" without the colour
     * having to carry it alone, which matters for anyone who reads green and
     * grey as one.
     */
    pill: [
      'flex h-8 w-14 items-center justify-center rounded-pill',
      'transition-[background-color,scale] duration-fast ease-standard',
      'group-aria-[current=page]/tab:bg-surface-selected',
      'group-aria-[current=page]/tab:text-fg-brand',
      'group-active/tab:scale-92 group-active/tab:bg-surface-hover',
      'motion-reduce:group-active/tab:scale-100',
    ],
    label: 'max-w-full truncate px-0.5 text-tab',
    badge: [
      'absolute start-1/2 top-1 ms-2 flex h-4 min-w-4 items-center justify-center',
      'rounded-pill bg-sev-error px-1 text-badge text-white tracking-normal',
      'ring-2 ring-panel',
    ],
  },
});

const styles = tabBar();

export interface TabBarProps extends ComponentPropsWithoutRef<'nav'> {
  children: ReactNode;
}

/** The bar. Three to five `TabBarItem`s; past five, the rest go under "More". */
export function TabBar({ children, className, ...props }: TabBarProps) {
  return (
    <nav className={styles.root({ className })} {...props}>
      {children}
    </nav>
  );
}

TabBar.displayName = 'TabBar';

export interface TabBarItemProps
  extends Omit<useRender.ComponentProps<'a'>, 'children'> {
  icon: IconComponent;
  label: string;
  /** The tab the page is under. Sets `aria-current="page"`. */
  active?: boolean;
  /**
   * A count worth leaving the page for: new issues, say. Capped at 99+, since
   * the dot's job is "there is something", not the exact figure.
   */
  badge?: number;
}

/**
 * One destination. A link by default; `render` turns it into the router's link,
 * or a `<button>` for a tab that opens something instead of going somewhere
 * (search, "More").
 */
export function TabBarItem({
  icon: Icon,
  label,
  active,
  badge,
  className,
  render,
  ...props
}: TabBarItemProps) {
  return useRender({
    defaultTagName: 'a',
    render,
    props: mergeProps<'a'>(
      {
        'aria-current': active ? 'page' : undefined,
        className: styles.item({ className }),
        children: (
          <>
            <span className={styles.pill()}>
              <Icon size="3xl" />
            </span>
            <span className={styles.label()}>{label}</span>
            {badge ? (
              <span className={styles.badge()} data-numeric="">
                {badge > 99 ? '99+' : badge}
              </span>
            ) : null}
          </>
        ),
      },
      props,
    ),
  });
}

TabBarItem.displayName = 'TabBarItem';
