'use client';

import { type ReactNode, useEffect } from 'react';
import { cn } from '../../lib/cn';
import { tv } from '../../lib/tv';
import { TooltipProvider } from '../tooltip/tooltip';
import {
  SidebarProvider,
  type SidebarProviderProps,
  useSidebar,
} from './sidebar-context';

export interface AppShellProps
  extends Omit<SidebarProviderProps, 'children' | 'shortcutKey'> {
  sidebar: ReactNode;
  /**
   * The phone's navigation (`TabBar`). Given one, below `md` the sidebar is
   * not drawn at all and the bar sits under the sheet instead; without one
   * the sidebar becomes a drawer behind a menu button, as before.
   */
  tabBar?: ReactNode;
  children: ReactNode;
  className?: string;
  /** The shortcut that collapses the sidebar. `false` turns it off. */
  collapseShortcut?: string | false;
}

/**
 * The application frame: the sidebar on the canvas, and beside it the sheet
 * the pages sit on -- inset by 8 px, rounded, one step lighter. The page's own
 * `Topbar` goes at the top of the sheet, as its first child.
 *
 * The inset is the hierarchy. The canvas around the edge is the application;
 * the sheet is the page. Nothing else is needed to say which is which, so the
 * sidebar carries no border and no fill of its own.
 *
 * On a phone the sheet goes edge to edge: 8 px of frame on a 375 px screen is
 * 4 % of the width spent on a border.
 *
 * It occupies exactly the window and the only thing that scrolls is the
 * content. An error tracker is a place you spend the day jumping between a list
 * and a record, and having the navigation scroll away while you read down a
 * thousand events means scrolling back up to leave.
 *
 * `h-dvh` and not `h-screen`: on a phone the browser chrome appears and
 * disappears as you scroll, and with `vh` the last row of a list sits
 * permanently underneath it.
 *
 * It brings the tooltip provider with it, because the collapsed sidebar depends
 * on it: without a provider, running down a rail of seven icons means waiting
 * out the full delay seven times.
 */
export function AppShell({
  sidebar,
  tabBar,
  children,
  className,
  collapseShortcut = 'b',
  ...sidebarState
}: AppShellProps) {
  return (
    <TooltipProvider>
      <SidebarProvider shortcutKey={collapseShortcut} {...sidebarState}>
        <div
          className={cn(
            'relative flex h-dvh overflow-hidden bg-canvas text-fg',
            tabBar ? 'max-md:flex-col' : undefined,
            className,
          )}
        >
          {tabBar ? (
            <div className="contents max-md:hidden">{sidebar}</div>
          ) : (
            <>
              {sidebar}
              <DrawerScrim />
            </>
          )}

          <main
            className={cn(
              'flex min-w-0 flex-1 flex-col overflow-hidden bg-panel',
              'md:my-frame md:me-frame md:rounded-3xl',
              'md:border md:border-border-subtle',
            )}
          >
            {children}
          </main>
          {tabBar ? <div className="contents md:hidden">{tabBar}</div> : null}
        </div>
      </SidebarProvider>
    </TooltipProvider>
  );
}

AppShell.displayName = 'AppShell';

/**
 * The dimmed backdrop behind the navigation drawer, on phones only.
 *
 * It does two jobs and both are needed: it dims the content so the navigation
 * reads as being on top of it, and it is the way out -- a tap outside closes,
 * which is what everyone tries before hunting for a cross. Escape does the same
 * for anyone on a keyboard.
 */
function DrawerScrim() {
  const { drawerOpen, setDrawerOpen } = useSidebar();

  useEffect(() => {
    if (!drawerOpen) {
      return;
    }

    function onKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') {
        setDrawerOpen(false);
      }
    }

    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [drawerOpen, setDrawerOpen]);

  return (
    <button
      type="button"
      tabIndex={-1}
      aria-hidden="true"
      onClick={() => setDrawerOpen(false)}
      data-open={drawerOpen || undefined}
      className={cn(
        'absolute inset-0 z-30 bg-scrim md:hidden',
        'transition-opacity duration-slow ease-standard',
        'not-data-open:pointer-events-none not-data-open:opacity-0',
      )}
    />
  );
}

export interface PageProps {
  children: ReactNode;
  className?: string;
  /**
   * The page scrolls as one. Off for a screen that owns its own scrolling --
   * a list with a sticky header and a pinned footer -- which is most of them.
   */
  scroll?: boolean;
}

/**
 * A page's content, with the system's 28 px gutter.
 *
 * When it scrolls it also takes focus, and that is not decoration: a logs
 * screen is sixty lines of text with nothing clickable in it, and a scrollable
 * box with no focusable content inside is unreachable with a keyboard --
 * there is literally no way to page down it. The cost is one extra tab stop
 * ahead of the content on pages that do have controls, which is the trade the
 * rule asks for and the one every code viewer makes.
 */
export function Page({ children, className, scroll = true }: PageProps) {
  return (
    <div
      tabIndex={scroll ? 0 : undefined}
      className={cn(
        'flex min-h-0 flex-1 flex-col gap-4 p-page-gutter outline-none',
        scroll ? 'overflow-y-auto' : 'overflow-hidden',
        /*
         * On a phone nothing scrolls inside the page: the shell's content
         * area scrolls it all, header included. A header that stays put
         * while a list scrolls under it costs a third of a 390 px screen for
         * the whole read; scrolled away, it is one flick back up.
         */
        'max-md:flex-none max-md:overflow-visible',
        className,
      )}
    >
      {children}
    </div>
  );
}

Page.displayName = 'Page';

/**
 * With tabs, their own rule is the header's bottom edge: a border here as
 * well would draw a second line under it, and the lime indicator would sit on
 * one and not the other.
 */
const pageHeader = tv({
  // Tighter on a phone, where the header is the fold's first third.
  base: 'shrink-0 px-page-gutter pt-7 max-md:pt-4',
  variants: {
    tabs: {
      true: '',
      false: 'border-b border-border-divider pb-6 max-md:pb-4',
    },
  },
});

export interface PageHeaderProps {
  title: ReactNode;
  /**
   * The page's state in a few facts, read left to right and separated by
   * dots: `3 projects`, `1 with fatal errors`, `65 events today`. Facts, not
   * a description: "Manage your projects" tells nobody anything twice.
   */
  summary?: ReactNode[];
  /** Pushed to the right, level with the title. */
  actions?: ReactNode;
  /** The page's tabs, under everything, sitting on the bottom rule. */
  tabs?: ReactNode;
  className?: string;
}

/**
 * A page's heading: what it is, how it stands, and what can be done to it.
 *
 * No icon and no box. The sidebar already says which route this is, and a
 * tile beside the title is the same signal said a second time, louder. What
 * the header adds instead is the summary -- the two or three facts someone
 * opened the page to learn.
 *
 * It ends on a rule the width of the sheet, with the tabs sitting on it when
 * there are any. It sits outside the page's scroller (`Page`), so it stays put
 * while the content moves under it: the title and the actions are always one
 * glance away, however far down the list you are.
 */
export function PageHeader({
  title,
  summary,
  actions,
  tabs,
  className,
}: PageHeaderProps) {
  // Not `filter(Boolean)`: a `0` is a fact worth reading.
  const facts =
    summary?.filter((fact) => fact != null && fact !== false && fact !== '') ??
    [];

  return (
    <div className={pageHeader({ tabs: Boolean(tabs), className })}>
      {/* Wraps instead of squeezing: on a phone the actions -- a page's
          filters, say -- drop under the title rather than crushing it. */}
      <div className="flex flex-wrap items-start justify-between gap-x-6 gap-y-4 max-md:gap-y-3">
        <div className="min-w-0 max-w-full">
          <h1 className="truncate text-page-title text-fg">{title}</h1>
          {facts.length > 0 ? (
            <p className="mt-1.5 flex min-w-0 flex-wrap items-center gap-x-2 text-value text-fg-subtle">
              {facts.map((fact, index) => (
                // Facts are positional: the order is the reading order, and
                // a fact is any node, so there is no identity to key on.
                // react-doctor-disable-next-line react-doctor/no-array-index-as-key
                <span key={index} className="flex items-center gap-2">
                  {/* On a phone the facts wrap, and a dot would lead the new line. */}
                  {index > 0 ? (
                    <span
                      aria-hidden="true"
                      className="hidden text-fg-ghost sm:inline"
                    >
                      ·
                    </span>
                  ) : null}
                  {fact}
                </span>
              ))}
            </p>
          ) : null}
        </div>
        {actions ? (
          <div
            className={cn(
              'flex min-w-0 max-w-full grow flex-wrap items-center justify-end gap-2',
              /*
               * On a phone the actions are one row of chips under the title,
               * scrolled sideways when there are more than fit -- the iOS
               * and Android filter row. Wrapped, four filters take three
               * lines of the screen before the first result.
               */
              'max-md:-mx-page-gutter max-md:w-[calc(100%+2*var(--spacing-page-gutter))] max-md:max-w-none',
              'max-md:flex-nowrap max-md:justify-start max-md:overflow-x-auto max-md:px-page-gutter',
              'max-md:-my-1 max-md:py-1 max-md:[scrollbar-width:none] max-md:*:shrink-0 max-md:*:flex-nowrap',
            )}
          >
            {actions}
          </div>
        ) : null}
      </div>
      {/* Out to the page's edges, so the rule runs as wide as a border would;
          the gutter moves inside the strip. */}
      {tabs ? (
        <div className="-mx-page-gutter mt-5 max-md:mt-3 [&_[data-slot=tab-frame]]:before:bg-border-divider [&_[data-slot=tab-strip]]:px-page-gutter">
          {tabs}
        </div>
      ) : null}
    </div>
  );
}

PageHeader.displayName = 'PageHeader';
