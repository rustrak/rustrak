import { Collapsible as BaseCollapsible } from '@base-ui/react/collapsible';
import { mergeProps } from '@base-ui/react/merge-props';
import { useRender } from '@base-ui/react/use-render';
import { type ComponentPropsWithoutRef, type ReactNode, useState } from 'react';
import { cn } from '../../lib/cn';
import { focusRing } from '../../lib/focus';
import {
  collapseFade,
  interactiveTransition,
  pressScaleTrigger,
  scopeEnter,
  scopeGroup,
  swapAnimation,
} from '../../lib/motion';
import { tv } from '../../lib/tv';
import { Avatar } from '../avatar/avatar';
import { Wordmark } from '../brand/wordmark';
import { Count } from '../count/count';
import type { IconComponent } from '../icon/icon';
import {
  ChevronRightIcon,
  MoreIcon,
  PanelLeftIcon,
  SearchIcon,
  SelectorIcon,
} from '../icon/icon-catalog';
import { Kbd } from '../kbd/kbd';
import { Menu, MenuActions } from '../menu/menu';
import type { MenuAction } from '../menu/menu-parts';
import { Separator } from '../separator/separator';
import { Text } from '../text/text';
import { Tooltip } from '../tooltip/tooltip';
import { useSidebar } from './sidebar-context';

/**
 * The sidebar: 256 px open, a 64 px rail collapsed.
 *
 * It is not a panel. It is drawn straight on the canvas -- no fill of its own,
 * no border -- and the pages sit beside it on a sheet that is one step
 * lighter. That is the whole hierarchy of the frame.
 *
 * Top to bottom: the mark and the collapse toggle, the scope card (which
 * project you are in, and the only way to change it), search, the routes of
 * that scope, and at the foot settings and the account.
 *
 * **Collapsing is one element, not two.** Every row keeps the same markup in
 * both states: the column narrows, the labels fade, and each icon sits where
 * it already was -- 12 px in, which is the centre of the 40 px rail. Swapping
 * to a second rail markup is what made the old collapse a jump cut.
 */
const sidebar = tv({
  base: [
    'flex w-sidebar shrink-0 flex-col overflow-hidden bg-canvas px-3 pt-3.5 pb-2.5',
    // Collapsing covers 180 px: the longest distance in the system, so it gets
    // the longest duration the system uses day to day.
    'transition-[width] duration-slow ease-standard',
    /*
     * On a phone it stops taking up space and becomes a drawer over the
     * content, moved with `translate` -- the only thing that does not lay the
     * page out again on every frame. `visibility` rides along so a closed
     * drawer leaves nothing focusable off screen.
     */
    'max-md:fixed max-md:inset-y-0 max-md:start-0 max-md:z-40',
    'max-md:shadow-overlay',
    'max-md:transition-[translate,visibility] max-md:duration-slow',
    'max-md:not-data-drawer-open:invisible',
    'max-md:not-data-drawer-open:-translate-x-full',
    // Crossing the breakpoint animates nothing: motion is for what somebody
    // asked for, not for what falls out of a resize.
    'data-switching:transition-none',
  ],
  variants: {
    collapsed: { true: 'md:w-sidebar-rail', false: 'md:w-sidebar' },
  },
});

/** A label that fades when the column narrows, and returns once it is wide. */
const label = tv({
  base: ['min-w-0 truncate', collapseFade],
  variants: {
    collapsed: {
      true: 'pointer-events-none opacity-0',
      false: 'opacity-100 delay-(--transition-duration-fast)',
    },
  },
});

export interface SidebarProps extends ComponentPropsWithoutRef<'nav'> {
  /** The mark, the scope card and search: above the routes, never scrolled. */
  header?: ReactNode;
  /** Pinned to the bottom: settings, docs and the account. */
  footer?: ReactNode;
}

export function Sidebar({
  className,
  header,
  children,
  footer,
  ...props
}: SidebarProps) {
  const { collapsed, drawerOpen, switching } = useSidebar();

  return (
    <nav
      aria-label="Main navigation"
      data-collapsed={collapsed || undefined}
      data-drawer-open={drawerOpen || undefined}
      data-switching={switching || undefined}
      className={sidebar({ collapsed, className })}
      {...props}
    >
      {header ? (
        <div className="flex shrink-0 flex-col gap-1.5 pb-2">{header}</div>
      ) : null}

      {/* The negative margin gives the focus ring room inside the scroller,
          which would otherwise clip it on both sides. */}
      <div className="-mx-1 flex min-h-0 flex-1 flex-col gap-px overflow-x-hidden overflow-y-auto px-1">
        {children}
      </div>

      {footer ? (
        <div className="flex shrink-0 flex-col gap-px pt-2.5">{footer}</div>
      ) : null}
    </nav>
  );
}

Sidebar.displayName = 'Sidebar';

/* ------------------------------------------------------------------------- */
/* The mark and the collapse toggle                                          */
/* ------------------------------------------------------------------------- */

export interface SidebarBrandProps
  extends Omit<useRender.ComponentProps<'a'>, 'children'> {
  /**
   * Takes the mark's place: the project switcher (`SidebarProject`), so the
   * first row says where you are rather than what the product is called.
   */
  switcher?: ReactNode;
}

/**
 * The top row: the mark -- or the project switcher in its place -- and the
 * toggle that collapses the sidebar.
 *
 * The toggle sits at the end and is exactly the rail's width, so collapsing
 * leaves it where the eye already is. Everything before it fades out rather
 * than being squeezed: it is never drawn smaller than its floor.
 */
export function SidebarBrand({
  className,
  render,
  switcher,
  ...props
}: SidebarBrandProps) {
  const { collapsed, toggle } = useSidebar();
  const toggleLabel = collapsed ? 'Expand sidebar' : 'Collapse sidebar';

  const mark = useRender({
    defaultTagName: 'a',
    render,
    props: mergeProps<'a'>(
      {
        'aria-label': 'Rustrak',
        'aria-hidden': collapsed || undefined,
        tabIndex: collapsed ? -1 : undefined,
        className: cn(
          'absolute start-2 top-0 flex h-7 items-center rounded-xs text-fg',
          'hover:text-fg-secondary',
          focusRing,
          label({ collapsed }),
          // `label` brings the fade; the colour change on hover rides with it.
          'transition-[opacity,color]',
        ),
        children: <Wordmark className="h-wordmark w-auto" />,
      },
      props,
    ),
  });

  return (
    <div
      className={cn(
        'relative flex shrink-0 justify-end',
        switcher ? 'h-10 pb-2' : 'h-9 pb-2',
        className,
      )}
    >
      {switcher ? (
        <div
          // Faded out on the rail, and out of the tab order with it.
          inert={collapsed}
          className={cn(
            'absolute start-0 top-0 end-rail-item-w flex min-w-0',
            label({ collapsed }),
          )}
        >
          {switcher}
        </div>
      ) : (
        mark
      )}
      <Tooltip content={`${toggleLabel} (⌘B)`} side="right">
        <button
          type="button"
          onClick={toggle}
          aria-label={toggleLabel}
          aria-expanded={!collapsed}
          className={cn(
            'flex shrink-0 items-center justify-center rounded-md',
            'text-fg-subtle hover:bg-surface-hover hover:text-fg',
            switcher ? 'h-8 w-rail-item-w' : 'h-7 w-rail-item-w',
            interactiveTransition,
            focusRing,
            'max-md:hidden',
          )}
        >
          <PanelLeftIcon size="2xl" />
        </button>
      </Tooltip>
    </div>
  );
}

SidebarBrand.displayName = 'SidebarBrand';

/* ------------------------------------------------------------------------- */
/* The scope card                                                            */
/* ------------------------------------------------------------------------- */

const projectCard = tv({
  base: [
    'flex h-nav-item max-w-full shrink-0 items-center gap-2 rounded-lg',
    'ps-3 pe-2 text-start',
    'hover:bg-surface-hover data-popup-open:bg-surface-active',
    interactiveTransition,
    pressScaleTrigger,
    focusRing,
  ],
});

export interface SidebarProjectProps
  extends Omit<useRender.ComponentProps<'button'>, 'children'> {
  /** The project, or the workspace when you are above any one of them. */
  name: string;
  /** The line under it: the slug, or how many projects there are. */
  detail?: string;
  /** The code inside the monogram, when there is no `tile`. */
  platform?: string;
  /** The project's logo, 26 px. Replaces the monogram. */
  tile?: ReactNode;
}

/**
 * The scope card: which project the routes below belong to, and the one
 * control that changes it. Hand it to a `Menu` as its trigger.
 *
 * It is the only switcher in the sidebar on purpose. A second way to pick a
 * project further down -- a list of them -- puts two answers to "where am I"
 * on screen at once, and the reader has to work out which one is current.
 */
export function SidebarProject({
  name,
  detail,
  platform,
  tile,
  className,
  render,
  ...props
}: SidebarProjectProps) {
  const { collapsed } = useSidebar();

  const element = useRender({
    defaultTagName: 'button',
    render,
    props: mergeProps<'button'>(
      {
        type: 'button',
        'aria-label': detail ? `${name} · ${detail}` : name,
        className: projectCard({ className }),
        children: (
          <>
            {/* Keyed by the name, so switching scope replays the swap: the
                tile and the name hand over in place. */}
            <span
              key={name}
              className={cn('flex size-avatar-xs shrink-0', swapAnimation)}
            >
              {tile ?? (
                <Avatar
                  shape="square"
                  size="xs"
                  tone="brand"
                  name={name}
                  initials={platform}
                />
              )}
            </span>
            <span className={cn('flex min-w-0 flex-col', label({ collapsed }))}>
              {/* One line, like every other row: the detail stays in the
                  accessible name and the menu, not under the name. */}
              <Text
                key={name}
                variant="value"
                truncate
                className={cn('font-medium', swapAnimation)}
              >
                {name}
              </Text>
            </span>
            <SelectorIcon
              size="md"
              className={cn('shrink-0 text-fg-subtle', label({ collapsed }))}
            />
          </>
        ),
      },
      props,
    ),
  });

  return (
    <Tooltip content={name} side="right" disabled={!collapsed}>
      {element}
    </Tooltip>
  );
}

SidebarProject.displayName = 'SidebarProject';

/* ------------------------------------------------------------------------- */
/* Routes                                                                    */
/* ------------------------------------------------------------------------- */

const row = tv({
  base: [
    // 12 px in, so the 16 px icon is centred in the 40 px rail and does not
    // move when the column narrows around it.
    'relative flex h-nav-item w-full shrink-0 items-center gap-2.5',
    'rounded-lg px-3 text-value text-fg-tertiary',
    'hover:bg-surface-hover hover:text-fg-secondary',
    interactiveTransition,
    scopeEnter,
    // A nav row does not sink on press: shrinking a full-width row reads as a
    // flicker. The background arriving already says it was pressed.
    focusRing,
  ],
  variants: {
    active: { true: 'bg-surface-active text-fg hover:bg-surface-active' },
  },
});

/** The count beside a label: plain, or on an amber chip when it is a warning. */
const rowCount = tv({
  base: 'rounded-sm px-1.5 leading-4.5',
  variants: {
    tone: {
      default: '',
      warning: 'bg-sev-warning-surface text-sev-warning-fg',
    },
  },
});

/** The dot a warning count turns into on the rail. */
const railDot = tv({
  base: [
    'absolute end-2 top-2 size-1.5 rounded-pill bg-sev-warning',
    collapseFade,
  ],
  variants: {
    collapsed: { true: 'opacity-100', false: 'opacity-0' },
  },
});

type SidebarItemMark =
  | { icon: IconComponent; leading?: never }
  /** A thing rather than a route -- a project -- drawn by its 18 px logo. */
  | { leading: ReactNode; icon?: never };

export type SidebarItemProps = SidebarItemMark &
  Omit<useRender.ComponentProps<'a'>, 'children'> & {
    label: string;
    /** How many there are. Zero is not drawn. */
    count?: number | string;
    /** A count that needs looking at -- a failing cron -- is drawn in amber. */
    tone?: 'default' | 'warning';
    active?: boolean;
    /** A dot at the end of the row: something here wants looking at. */
    status?: 'error' | 'warning' | 'info' | 'brand';
  };

const statusDot = tv({
  base: 'size-1.5 shrink-0 rounded-pill',
  variants: {
    status: {
      error: 'bg-sev-error',
      warning: 'bg-sev-warning',
      info: 'bg-chart-1',
      brand: 'bg-surface-brand',
    },
  },
});

/**
 * A route.
 *
 * With `render` the element changes without losing the look, which is how a
 * router's link is plugged in: `render={<Link to="/issues" />}`.
 *
 * Collapsed, the label fades and moves into a tooltip; a warning count
 * becomes a dot, because "something here needs looking at" is the part that
 * survives the loss of width.
 */
export function SidebarItem({
  icon: Icon,
  leading,
  label: text,
  count,
  tone = 'default',
  active,
  status,
  className,
  render,
  ...props
}: SidebarItemProps) {
  const { collapsed } = useSidebar();
  const hasCount = count != null && count !== 0 && count !== '';

  const element = useRender({
    defaultTagName: 'a',
    render,
    props: mergeProps<'a'>(
      {
        'aria-current': active ? 'page' : undefined,
        'aria-label': collapsed ? text : undefined,
        className: row({ active, className }),
        children: (
          <>
            {Icon ? <Icon size="2xl" className="shrink-0" /> : leading}
            <span className={cn('flex-1', label({ collapsed }))}>{text}</span>
            <Count
              tone={tone === 'warning' ? 'strong' : 'default'}
              className={cn(rowCount({ tone }), label({ collapsed }))}
            >
              {count}
            </Count>
            {status ? (
              <span
                aria-hidden="true"
                className={cn(statusDot({ status }), label({ collapsed }))}
              />
            ) : null}
            {hasCount && tone === 'warning' ? (
              <span aria-hidden="true" className={railDot({ collapsed })} />
            ) : null}
          </>
        ),
      },
      props,
    ),
  });

  return (
    <Tooltip
      content={hasCount ? `${text} · ${count}` : text}
      side="right"
      disabled={!collapsed}
    >
      {element}
    </Tooltip>
  );
}

SidebarItem.displayName = 'SidebarItem';

/** The hairline a group's label turns into on the rail. */
const groupRule = tv({
  base: ['absolute start-2 top-1/2 w-6', collapseFade],
  variants: {
    collapsed: { true: 'opacity-100', false: 'opacity-0' },
  },
});

export interface SidebarGroupProps {
  /** What the routes underneath have in common: Triage, Monitor. */
  label: string;
  children: ReactNode;
}

/**
 * Routes that belong together, under a quiet label.
 *
 * Collapsed, the label fades and a short hairline fades in where it was, in
 * the same space: the rail still shows where one group ends, and nothing
 * below it moves up or down.
 */
export function SidebarGroup({ label: text, children }: SidebarGroupProps) {
  const { collapsed } = useSidebar();

  return (
    <div className={cn('flex shrink-0 flex-col gap-px', scopeGroup)}>
      <div
        className={cn(
          'relative mt-4 flex h-7 shrink-0 items-center px-2.5',
          'text-label font-normal text-fg-subtle',
          scopeEnter,
        )}
      >
        <span className={label({ collapsed })}>{text}</span>
        <Separator className={groupRule({ collapsed })} />
      </div>
      {children}
    </div>
  );
}

SidebarGroup.displayName = 'SidebarGroup';

export interface SidebarSubmenuProps {
  /** The thing the routes inside belong to: a project's name. */
  label: string;
  /** Its 18 px logo. */
  leading: ReactNode;
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  /** The routes inside: `SidebarItem`s. */
  children: ReactNode;
}

/**
 * A row that opens to the routes of one thing -- a project's settings -- set
 * in under it on a hairline, so they read as belonging to it.
 *
 * The panel's height animates (Base UI measures it into
 * `--collapsible-panel-height`), so the rows below slide instead of jumping.
 * On the rail there is no room for nested rows: the logo stays, with the name
 * in its tooltip, and the panel is left closed.
 */
export function SidebarSubmenu({
  label: text,
  leading,
  open,
  defaultOpen,
  onOpenChange,
  children,
}: SidebarSubmenuProps) {
  const { collapsed } = useSidebar();

  return (
    <BaseCollapsible.Root
      open={collapsed ? false : open}
      defaultOpen={defaultOpen}
      onOpenChange={onOpenChange}
      className="flex shrink-0 flex-col"
    >
      <Tooltip content={text} side="right" disabled={!collapsed}>
        <BaseCollapsible.Trigger
          aria-label={collapsed ? text : undefined}
          className={row({ className: 'group/submenu' })}
        >
          {leading}
          <span className={cn('flex-1 text-start', label({ collapsed }))}>
            {text}
          </span>
          <ChevronRightIcon
            size="md"
            aria-hidden="true"
            className={cn(
              'shrink-0 text-fg-ghost transition-[rotate] duration-moderate ease-standard',
              'group-data-panel-open/submenu:rotate-90',
              'motion-reduce:transition-none',
              label({ collapsed }),
            )}
          />
        </BaseCollapsible.Trigger>
      </Tooltip>
      <BaseCollapsible.Panel
        className={cn(
          'h-(--collapsible-panel-height) overflow-hidden',
          'transition-[height] duration-moderate ease-standard',
          'data-starting-style:h-0 data-ending-style:h-0',
          'motion-reduce:transition-none',
        )}
      >
        <div className="ms-5 mt-px mb-1.5 flex flex-col gap-px border-s border-border-subtle ps-2">
          {children}
        </div>
      </BaseCollapsible.Panel>
    </BaseCollapsible.Root>
  );
}

SidebarSubmenu.displayName = 'SidebarSubmenu';

type Direction = 'forward' | 'back' | 'swap';

export interface SidebarScopeProps {
  /**
   * What the routes below are about: `workspace`, `project:42`. A new value
   * replays the entrance.
   */
  scope: string;
  /**
   * How deep the scope is: 0 for the workspace, 1 inside a project. Going
   * deeper slides the rows in from the right, coming back from the left, and
   * a move at the same depth -- one project to another -- swaps in place.
   */
  depth: number;
  children: ReactNode;
}

/**
 * The routes of one scope, and the entrance when the scope changes.
 *
 * Moving between the workspace and a project is the one structural change the
 * sidebar makes, and done in a single frame it reads as the navigation
 * breaking. Animated with a direction it reads as going in and coming back
 * out, which is what happened. The rows build top to bottom, each a beat after
 * the one above.
 *
 * The first paint does not animate: nothing changed, the page just arrived.
 */
export function SidebarScope({ scope, depth, children }: SidebarScopeProps) {
  const [last, setLast] = useState<{
    scope: string;
    depth: number;
    direction?: Direction;
  }>({ scope, depth });

  // Adjusting state while rendering, so the rows mount already knowing which
  // way to come in -- an effect would paint them once in place first.
  if (last.scope !== scope) {
    setLast({
      scope,
      depth,
      direction:
        depth > last.depth ? 'forward' : depth < last.depth ? 'back' : 'swap',
    });
  }

  return (
    <div
      key={scope}
      data-direction={last.direction}
      // Empty -- the workspace has no project card -- it takes no room, so
      // the gap around it does not either.
      className="group/scope flex flex-col gap-px empty:hidden"
    >
      {children}
    </div>
  );
}

SidebarScope.displayName = 'SidebarScope';

/* ------------------------------------------------------------------------- */
/* Search                                                                    */
/* ------------------------------------------------------------------------- */

/**
 * A field, not a route: its own box, a step lighter than the sidebar with a
 * hairline, so it cannot be mistaken for one more link. On the rail the box
 * dissolves and the lens is left in line with the route icons.
 */
const searchField = tv({
  base: [
    'relative mb-2 flex h-nav-item w-full shrink-0 items-center gap-2.5',
    'rounded-lg border px-2.75 text-value text-fg-meta',
    'hover:text-fg-subtle',
    interactiveTransition,
    focusRing,
  ],
  variants: {
    collapsed: {
      true: 'border-transparent hover:bg-surface-hover',
      false: 'border-border-subtle bg-panel hover:border-border',
    },
  },
});

export interface SidebarSearchProps
  extends Omit<ComponentPropsWithoutRef<'button'>, 'children'> {
  label?: string;
  /** The shortcut that opens it. */
  shortcut?: string;
}

/**
 * Global search.
 *
 * A button, not an `input`: it opens the command palette rather than
 * searching in place. Drawn as one more row so it reads as a place you go,
 * which is what the palette is.
 */
export function SidebarSearch({
  label: text = 'Search',
  shortcut = '⌘K',
  className,
  ...props
}: SidebarSearchProps) {
  const { collapsed } = useSidebar();

  return (
    <Tooltip
      content={`${text} (${shortcut})`}
      side="right"
      disabled={!collapsed}
    >
      <button
        type="button"
        aria-label={text}
        className={cn(searchField({ collapsed }), className)}
        {...props}
      >
        <SearchIcon size="xl" className="shrink-0" />
        <span className={cn('flex-1 text-start', label({ collapsed }))}>
          {text}
        </span>
        <Kbd className={label({ collapsed })}>{shortcut}</Kbd>
      </button>
    </Tooltip>
  );
}

SidebarSearch.displayName = 'SidebarSearch';

/* ------------------------------------------------------------------------- */
/* The account                                                               */
/* ------------------------------------------------------------------------- */

const user = tv({
  slots: {
    trigger: [
      // 7 px in, so the 26 px avatar is centred in the 40 px rail.
      'flex w-full shrink-0 items-center gap-2.5 rounded-lg p-1.75 text-start',
      'hover:bg-surface-hover data-popup-open:bg-surface-hover',
      interactiveTransition,
      pressScaleTrigger,
      focusRing,
    ],
    popup: 'w-64 max-w-(--available-width)',
    header: 'flex items-center gap-2.5 px-2.5 pt-2.5 pb-3',
    footer: [
      'flex items-center justify-between gap-3 px-2.5 pt-2 pb-1',
      'font-mono text-mono-sm text-fg-ghost',
    ],
  },
});

const userStyles = user();

export interface SidebarUserProps {
  name: string;
  /** The line under the name: the role, or the address. */
  detail?: string;
  email?: string;
  src?: string;
  /** What can be done from here. Sign out goes last and separated. */
  actions: MenuAction[];
  /** The build. Not an action: it is what you read out over the phone. */
  version?: string;
}

/**
 * The account, at the foot of the sidebar.
 *
 * The whole row opens the menu, and it ends in an ellipsis because a name on
 * its own does not say there is anything behind it. Inside, the first thing is
 * who you are, and the last thing, separated, is leaving.
 */
export function SidebarUser({
  name,
  detail,
  email,
  src,
  actions,
  version,
}: SidebarUserProps) {
  const { collapsed } = useSidebar();

  return (
    <div className="mt-1 w-full">
      <Menu
        side={collapsed ? 'right' : 'top'}
        align={collapsed ? 'end' : 'start'}
        popupClassName={userStyles.popup()}
        trigger={
          <button
            type="button"
            aria-label={`Account: ${name}`}
            className={userStyles.trigger()}
          >
            <Avatar name={name} src={src} />
            <span className={cn('flex flex-1 flex-col', label({ collapsed }))}>
              <Text variant="value" truncate className="font-medium">
                {name}
              </Text>
              {detail ? (
                <Text variant="hint" tone="meta" truncate>
                  {detail}
                </Text>
              ) : null}
            </span>
            <MoreIcon
              size="lg"
              className={cn('shrink-0 text-fg-meta', label({ collapsed }))}
            />
          </button>
        }
      >
        <div className={userStyles.header()}>
          <Avatar name={name} src={src} size="md" />
          {/* `min-w-0` on the column and `w-full` on each line: that is what
              lets a long address clip instead of widening the panel. */}
          <span className="flex min-w-0 flex-1 flex-col items-start">
            <Text variant="card-title" truncate className="w-full">
              {name}
            </Text>
            {email ? (
              <Text variant="meta" tone="meta" truncate className="w-full">
                {email}
              </Text>
            ) : null}
          </span>
        </div>

        <Separator className="mx-2 mb-1.5" />

        <MenuActions actions={actions} />

        {version ? (
          <>
            <Separator className="mx-2 mt-1.5" />
            <div className={userStyles.footer()}>
              <span className="min-w-0 truncate">{version}</span>
            </div>
          </>
        ) : null}
      </Menu>
    </div>
  );
}

SidebarUser.displayName = 'SidebarUser';
