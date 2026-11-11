import { Menu as BaseMenu } from '@base-ui/react/menu';
import type { ReactNode } from 'react';
import { focusRingInset } from '../../lib/focus';
import { chevronFlip, interactiveTransition } from '../../lib/motion';
import { PANEL_OFFSET } from '../../lib/panel';
import { tv, type VariantProps } from '../../lib/tv';
import type { IconComponent } from '../icon/icon';
import { ChevronDownIcon } from '../icon/icon-catalog';
import { MenuActions } from '../menu/menu';
import {
  type MenuAction,
  menuPopupClass,
  menuPositionerClass,
} from '../menu/menu-parts';

/**
 * One action plus its variations: Resolve, and behind the chevron "Resolve in
 * the next release", "Resolve until it happens again".
 *
 * The split earns its complexity in exactly this situation: the common case is
 * one click away, and the four rarer ones are one click away from that, without
 * either of them costing the other any width. If the leading half is not the
 * answer nine times out of ten, this is a menu button and not a split button.
 *
 * The divider is drawn in the foreground colour at low opacity rather than in a
 * border token, so it works on lime and on a surface alike: the one thing it
 * must never do is look like the gap between two separate buttons.
 *
 * ## The press belongs to the whole control, not to the half you hit
 *
 * The obvious thing is to sink whichever half was pressed, and it is wrong: the
 * two halves are one pill, and shrinking one of them tears the seam open and
 * leaves the divider floating in a gap. So the *root* sinks, driven by
 * `has-[:active]` from either half. Pressing the label and pressing the chevron
 * move the same object, which is what it looks like.
 *
 * The same rule covers the state Base UI leaves behind: it opens on pointer-down
 * and hands capture to the popup, so `:active` never lands on the chevron. It
 * does set `data-pressed`, and it holds it for as long as the menu is open, so
 * the control stays sunk the whole time it is holding something open -- and the
 * chevron turns over to say the same thing twice.
 *
 * The halves take `focusRingInset` and not the usual ring, for the same reason
 * the root clips: an outward `box-shadow` on a child that reaches the pill's
 * edge is cut off by that clip, so the ring would show on two sides only.
 */
const split = tv({
  slots: {
    root: [
      'inline-flex shrink-0 overflow-hidden rounded-md',
      interactiveTransition,
      // The whole pill sinks, from either half. `has-*` is what keeps the seam
      // shut; sinking the halves independently tears it open.
      'has-[:active]:scale-97 motion-reduce:has-[:active]:scale-100',
      'has-data-pressed:scale-97 motion-reduce:has-data-pressed:scale-100',
      'has-disabled:has-[:active]:scale-100',
    ],
    action: [
      'flex items-center gap-1.75 px-3 text-control whitespace-nowrap',
      'select-none disabled:pointer-events-none',
      interactiveTransition,
      focusRingInset,
    ],
    trigger: [
      // `group` so the chevron can read `data-popup-open`.
      'group flex w-control-xs items-center justify-center',
      'border-s border-current/20',
      'select-none disabled:pointer-events-none',
      interactiveTransition,
      focusRingInset,
    ],
    chevron: ['shrink-0', chevronFlip],
  },
  variants: {
    variant: {
      primary: {
        action: [
          'bg-surface-brand font-semibold text-fg-on-brand',
          'hover:bg-surface-brand-hover',
          'disabled:bg-surface-disabled disabled:text-fg-disabled',
        ],
        trigger: [
          'bg-surface-brand text-fg-on-brand hover:bg-surface-brand-hover',
          // Held open, the chevron half stays lit: it is the half that is doing
          // something right now.
          'data-popup-open:bg-surface-brand-hover',
          'disabled:bg-surface-disabled disabled:text-fg-disabled',
        ],
      },
      secondary: {
        root: 'border border-border',
        action: [
          'bg-surface text-fg-secondary hover:bg-surface-hover hover:text-fg',
          'disabled:text-fg-disabled',
        ],
        trigger: [
          'bg-surface text-fg-subtle hover:bg-surface-hover hover:text-fg',
          'data-popup-open:bg-surface-hover data-popup-open:text-fg',
          'disabled:text-fg-disabled',
        ],
      },
    },
    size: {
      sm: { root: 'h-control-sm' },
      md: { root: 'h-control-md' },
      lg: { root: 'h-control-lg' },
    },
  },
  defaultVariants: { variant: 'primary', size: 'md' },
});

export interface SplitButtonProps extends VariantProps<typeof split> {
  /** The one action that is right nine times out of ten. */
  children: ReactNode;
  icon?: IconComponent;
  onClick?: () => void;
  /** What the chevron opens. Never empty: a split with no menu is a button. */
  actions: MenuAction[];
  /** Names the chevron for a screen reader: "More ways to resolve". */
  menuLabel: string;
  disabled?: boolean;
  className?: string;
}

export function SplitButton({
  children,
  icon: Icon,
  onClick,
  actions,
  menuLabel,
  variant,
  size,
  disabled,
  className,
}: SplitButtonProps) {
  const styles = split({ variant, size });

  return (
    <div className={styles.root({ className })}>
      <button
        type="button"
        onClick={onClick}
        disabled={disabled}
        className={styles.action()}
      >
        {Icon ? <Icon size="lg" /> : null}
        {children}
      </button>

      <BaseMenu.Root>
        <BaseMenu.Trigger
          aria-label={menuLabel}
          disabled={disabled}
          className={styles.trigger()}
        >
          <ChevronDownIcon size="sm" className={styles.chevron()} />
        </BaseMenu.Trigger>
        <BaseMenu.Portal>
          <BaseMenu.Positioner
            side="bottom"
            align="end"
            sideOffset={PANEL_OFFSET}
            className={menuPositionerClass()}
          >
            <BaseMenu.Popup className={menuPopupClass()}>
              <MenuActions actions={actions} />
            </BaseMenu.Popup>
          </BaseMenu.Positioner>
        </BaseMenu.Portal>
      </BaseMenu.Root>
    </div>
  );
}

SplitButton.displayName = 'SplitButton';
