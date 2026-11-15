import { Drawer } from '@base-ui/react/drawer';
import { mergeProps } from '@base-ui/react/merge-props';
import { useRender } from '@base-ui/react/use-render';
import type { ReactNode } from 'react';
import { focusRing, focusRingInset } from '../../lib/focus';
import { interactiveTransition, pressScaleSmall } from '../../lib/motion';
import { tv } from '../../lib/tv';
import type { IconComponent } from '../icon/icon';
import {
  ChevronRightIcon,
  CloseIcon,
  SelectedIcon,
} from '../icon/icon-catalog';

/**
 * The bottom sheet: what a phone opens where a desktop opens a menu or a
 * popover. Base UI's Drawer underneath, so it is swiped down to close, traps
 * focus, locks the page's scroll, closes on Escape and on a tap outside, and
 * gives focus back to what opened it.
 *
 * It rises from the bottom edge because that is where the thumb already is: a
 * list of destinations at the top of a 6.7" screen is a stretch, the same list
 * in a sheet is under the finger that opened it. It stops short of the top
 * (`max-h`), so the page behind stays visible as the thing you return to.
 *
 * The motion is the system's `slow` step on the entrance curve, the same 240 ms
 * the sidebar uses to slide; a swipe hands the timing to the finger and the
 * release finishes it at the speed it was thrown (`--drawer-swipe-strength`).
 */
const sheet = tv({
  slots: {
    backdrop: [
      'fixed inset-0 z-50 bg-scrim',
      'opacity-[calc(1-var(--drawer-swipe-progress,0))]',
      'transition-[opacity] duration-slow ease-standard',
      'data-starting-style:opacity-0 data-ending-style:opacity-0',
      'data-swiping:duration-0',
    ],
    viewport: 'fixed inset-0 z-50 flex items-end justify-center',
    popup: [
      'relative flex max-h-[calc(100dvh-var(--spacing-safe-top)-var(--spacing-topbar))] w-full flex-col',
      'rounded-t-3xl border-t border-border-subtle bg-surface shadow-dialog',
      'pb-safe-bottom outline-none md:max-w-dialog-md md:mb-6 md:rounded-3xl md:border',
      'translate-y-[var(--drawer-swipe-movement-y,0px)]',
      'transition-[translate] duration-slow ease-entrance',
      'data-starting-style:translate-y-full data-ending-style:translate-y-full',
      'data-ending-style:ease-exit',
      'data-ending-style:duration-[calc(var(--drawer-swipe-strength,1)*var(--transition-duration-slow))]',
      'data-swiping:duration-0 data-swiping:select-none',
      'motion-reduce:transition-none',
    ],
    handle: 'mx-auto mt-2 mb-1 h-1 w-9 shrink-0 rounded-pill bg-border-strong',
    header: 'flex shrink-0 items-center gap-3 px-4 pt-1 pb-2',
    title: 'min-w-0 flex-1 truncate text-section text-fg',
    close: [
      'flex size-control-sm shrink-0 items-center justify-center rounded-pill',
      'bg-surface-raised text-fg-subtle hover:text-fg',
      interactiveTransition,
      pressScaleSmall,
      focusRing,
    ],
    body: 'min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 pb-4',
  },
});

const styles = sheet();

export interface SheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  /** "Close", in the reader's language, for the round button by the title. */
  closeLabel: string;
  /** Left of the title: a back chevron when the sheet has drilled in. */
  leading?: ReactNode;
  children: ReactNode;
  className?: string;
}

export function Sheet({
  open,
  onOpenChange,
  title,
  closeLabel,
  leading,
  children,
  className,
}: SheetProps) {
  return (
    <Drawer.Root open={open} onOpenChange={onOpenChange}>
      <Drawer.Portal>
        <Drawer.Backdrop className={styles.backdrop()} />
        <Drawer.Viewport className={styles.viewport()}>
          <Drawer.Popup className={styles.popup({ className })}>
            <div aria-hidden="true" className={styles.handle()} />
            <div className={styles.header()}>
              {leading}
              <Drawer.Title className={styles.title()}>{title}</Drawer.Title>
              <Drawer.Close aria-label={closeLabel} className={styles.close()}>
                <CloseIcon size="xl" />
              </Drawer.Close>
            </div>
            <Drawer.Content className={styles.body()}>
              {children}
            </Drawer.Content>
          </Drawer.Popup>
        </Drawer.Viewport>
      </Drawer.Portal>
    </Drawer.Root>
  );
}

Sheet.displayName = 'Sheet';

/*
 * Inside a sheet: the inset grouped list of iOS Settings and Android's
 * preference screens. A label above, then the rows on one rounded card with
 * hairlines between them. The card is what groups them; the label only names
 * the group.
 */
const list = tv({
  slots: {
    section: 'mt-4 first:mt-1',
    sectionLabel: 'mb-1.5 px-3 text-label text-fg-subtle',
    card: [
      'overflow-hidden rounded-2xl bg-surface-raised',
      '[&>*+*]:border-t [&>*+*]:border-border-subtle',
    ],
    row: [
      'flex min-h-menu-item w-full items-center gap-3 px-3 py-2 text-start',
      'text-value text-fg outline-none',
      'active:bg-surface-hover aria-[current=page]:bg-surface-selected',
      interactiveTransition,
      focusRingInset,
    ],
    rowIcon: 'flex size-7 shrink-0 items-center justify-center text-fg-subtle',
    rowText: 'flex min-w-0 flex-1 flex-col',
    rowLabel: 'truncate',
    rowDetail: 'truncate text-meta text-fg-subtle',
    trailing: 'shrink-0 text-fg-ghost',
  },
  variants: {
    danger: { true: { row: 'text-danger-fg', rowIcon: 'text-danger-fg' } },
  },
});

const listStyles = list();

export function SheetSection({
  label,
  children,
}: {
  label?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className={listStyles.section()}>
      {label ? <h3 className={listStyles.sectionLabel()}>{label}</h3> : null}
      <div className={listStyles.card()}>{children}</div>
    </section>
  );
}

SheetSection.displayName = 'SheetSection';

export interface SheetRowProps
  extends Omit<useRender.ComponentProps<'button'>, 'children'> {
  icon?: IconComponent;
  /** Instead of an icon: a project's logo, an avatar. */
  leading?: ReactNode;
  label: ReactNode;
  /** A second line: a slug, an email, what the row will do. */
  detail?: ReactNode;
  /** The page the row leads to is the one open. */
  active?: boolean;
  /** Ticked: the chosen option in a list of options. */
  selected?: boolean;
  /** Leads somewhere deeper: a chevron, the iOS signal for "drills in". */
  chevron?: boolean;
  /** Something that cannot be undone, or signs you out. */
  danger?: boolean;
  trailing?: ReactNode;
}

/**
 * One row. A `<button>` by default; `render` makes it the router's link.
 */
export function SheetRow({
  icon: Icon,
  leading,
  label,
  detail,
  active,
  selected,
  chevron,
  danger,
  trailing,
  className,
  render,
  ...props
}: SheetRowProps) {
  return useRender({
    defaultTagName: 'button',
    render,
    props: mergeProps<'button'>(
      {
        type: render ? undefined : 'button',
        'aria-current': active ? 'page' : undefined,
        className: list({ danger }).row({ className }),
        children: (
          <>
            {Icon || leading ? (
              <span className={list({ danger }).rowIcon()}>
                {Icon ? <Icon size="2xl" /> : leading}
              </span>
            ) : null}
            <span className={listStyles.rowText()}>
              <span className={listStyles.rowLabel()}>{label}</span>
              {detail ? (
                <span className={listStyles.rowDetail()}>{detail}</span>
              ) : null}
            </span>
            {trailing}
            {selected ? (
              <SelectedIcon size="2xl" className="shrink-0 text-fg-brand" />
            ) : null}
            {chevron ? (
              <ChevronRightIcon size="xl" className={listStyles.trailing()} />
            ) : null}
          </>
        ),
      },
      props,
    ),
  });
}

SheetRow.displayName = 'SheetRow';
