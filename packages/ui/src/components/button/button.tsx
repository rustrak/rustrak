import { mergeProps } from '@base-ui/react/merge-props';
import { useRender } from '@base-ui/react/use-render';
import type { ReactNode } from 'react';
import { cn } from '../../lib/cn';
import { focusRing } from '../../lib/focus';
import {
  chevronFlip,
  interactiveTransition,
  pressScaleSmall,
  pressScaleTrigger,
} from '../../lib/motion';
import { tv, type VariantProps } from '../../lib/tv';
import type { IconComponent } from '../icon/icon';
import { ChevronDownIcon, SpinnerIcon } from '../icon/icon-catalog';
import { Kbd } from '../kbd/kbd';

/**
 * The system's button.
 *
 * Five looks, and they are not interchangeable:
 *
 *   primary        lime. One per screen, and it is the thing the screen is
 *                  for: Resolve, New project. Lime is the only saturated
 *                  colour the product spends on something that is not an
 *                  alarm, so spending it twice on one screen spends it on
 *                  nothing.
 *   secondary      the alternative that stands next to the primary. A border
 *                  and a surface, no fill.
 *   ghost          no border and no surface until you point at it. For the
 *                  topbar and for dense rows, where a grid of outlines would
 *                  read as a table.
 *   danger         destructive, always behind a confirmation.
 *   danger-primary destructive *and* primary, only inside the confirmation
 *                  that has already said what it takes with it. In a toolbar a
 *                  filled red button gets pressed before it gets read.
 *
 * Two rules inherited from the toolbar and not negotiable, which is why they
 * sit in the base: the label never wraps, and the button never shrinks.
 */
const button = tv({
  base: [
    // `group` so the chevron can read `data-popup-open` when Base UI turns
    // this button into a menu trigger.
    'group relative inline-flex shrink-0 items-center justify-center',
    'gap-1.75 rounded-md whitespace-nowrap text-control',
    'select-none disabled:pointer-events-none',
    // Pressing does more than darken: the button sinks. That is the signal
    // that separates "I pressed it" from "the pointer went over it".
    //
    // The trigger variant, because the same component is also what opens a
    // menu, and Base UI swallows `:active` on anything that opens a popup.
    interactiveTransition,
    pressScaleTrigger,
    // Disabled and loading do not sink: nothing happened worth acknowledging.
    'disabled:active:scale-100 disabled:data-pressed:scale-100',
    focusRing,
  ],
  variants: {
    variant: {
      primary: [
        'border border-surface-brand bg-surface-brand text-fg-on-brand',
        'font-semibold',
        'hover:border-surface-brand-hover hover:bg-surface-brand-hover',
        'disabled:border-transparent disabled:bg-surface-disabled',
        'disabled:text-fg-disabled',
      ],
      secondary: [
        'border border-border bg-surface text-fg-secondary',
        'hover:border-border-strong hover:text-fg',
        'disabled:border-border-subtle disabled:bg-surface',
        'disabled:text-fg-disabled',
      ],
      ghost: [
        'border border-transparent bg-transparent text-fg-subtle',
        'hover:bg-surface-hover hover:text-fg',
        'disabled:text-fg-disabled',
      ],
      danger: [
        'border border-border bg-surface text-danger-fg',
        'hover:border-danger hover:bg-danger-surface',
        'disabled:border-border-subtle disabled:text-fg-disabled',
      ],
      'danger-primary': [
        'border border-danger bg-danger text-white font-semibold',
        'hover:border-danger-fg hover:bg-danger-fg',
        'disabled:border-transparent disabled:bg-surface-disabled',
        'disabled:text-fg-disabled',
      ],
    },
    size: {
      xs: 'h-control-xs rounded-sm px-2',
      sm: 'h-control-sm px-2.5',
      md: 'h-control-md px-3',
      lg: 'h-control-lg px-3',
    },
    /** No label: the button becomes square and the type demands `aria-label`. */
    iconOnly: { true: 'px-0' },
    /**
     * Held down: not focus and not hover, but "this is on right now" -- a
     * filter that is applied, a panel that is open, the sort direction in use.
     *
     * Only `secondary` and `ghost` honour it. A primary is already the screen's
     * main action; it cannot additionally be switched on.
     */
    selected: { true: '' },
    loading: { true: 'pointer-events-none' },
  },
  compoundVariants: [
    { iconOnly: true, size: 'xs', class: 'w-control-xs' },
    { iconOnly: true, size: 'sm', class: 'w-control-sm' },
    { iconOnly: true, size: 'md', class: 'w-control-md' },
    { iconOnly: true, size: 'lg', class: 'w-control-lg' },
    /*
     * The small square sinks further.
     *
     * 3 % is a proportion, not a distance: on a 120 px button it is nearly
     * 4 px and reads; on the 26 px square in a row it is 0.8 px and nobody
     * notices. What has to stay constant is what is perceived, so the small
     * pieces get the short press.
     */
    { iconOnly: true, size: ['xs', 'sm'], class: pressScaleSmall },
    {
      variant: ['secondary', 'ghost'],
      selected: true,
      // The hover states are restated, not inherited: the variant's own
      // `hover:` would otherwise swap the brand border back to the plain one,
      // and a switched-on button under the pointer would look switched off.
      class: [
        'border-border-brand bg-surface-selected text-fg font-semibold',
        'hover:border-border-brand hover:bg-surface-active hover:text-fg',
      ],
    },
  ],
  defaultVariants: { variant: 'secondary', size: 'md' },
});

export type ButtonVariant = NonNullable<VariantProps<typeof button>['variant']>;
export type ButtonSize = NonNullable<VariantProps<typeof button>['size']>;

interface ButtonOwnProps
  extends Omit<useRender.ComponentProps<'button'>, 'children'>,
    Omit<VariantProps<typeof button>, 'iconOnly'> {
  /** The icon to the left of the label. */
  icon?: IconComponent;
  /** The shortcut that fires the same action, shown last: `⌘K`, `R`, `⏎`. */
  shortcut?: string;
  /**
   * Marks the button as a folder: it runs nothing, it opens a menu. Draws the
   * chevron on the right.
   */
  menu?: boolean;
  /** Swaps the icon for a spin and blocks interaction. */
  loading?: boolean;
}

/**
 * A button with no label is required to carry an `aria-label`, and the type
 * enforces it: in this design the icon-only button always sits under a tooltip,
 * and without an accessible name there is nothing to announce.
 */
export type ButtonProps = ButtonOwnProps &
  (
    | { children: ReactNode; 'aria-label'?: string }
    | { children?: undefined; 'aria-label': string }
  );

export function Button({
  variant,
  size,
  selected,
  loading,
  icon: Icon,
  shortcut,
  menu,
  className,
  render,
  children,
  disabled,
  ...props
}: ButtonProps) {
  const iconOnly = children == null;
  const iconSize = size === 'xs' ? 'md' : 'lg';
  const LeadingIcon = loading ? SpinnerIcon : Icon;

  return useRender({
    defaultTagName: 'button',
    render,
    props: mergeProps<'button'>(
      {
        type: 'button',
        disabled: disabled || loading,
        'aria-busy': loading || undefined,
        // `selected || undefined` would drop the attribute for the off state,
        // so a caller driving a filter with `selected={isOn}` would expose a
        // toggle while on and a plain button while off. Absent only when the
        // prop was never passed.
        'aria-pressed': selected ?? undefined,
        className: button({
          variant,
          size,
          selected,
          loading,
          iconOnly,
          className,
        }),
        children: (
          <>
            {LeadingIcon ? (
              <LeadingIcon
                size={iconSize}
                className={
                  loading
                    ? 'animate-spin motion-reduce:animate-none'
                    : undefined
                }
              />
            ) : null}
            {children}
            {shortcut ? (
              <Kbd
                tone={
                  variant === 'primary' || variant === 'danger-primary'
                    ? 'on-brand'
                    : 'default'
                }
                // A key to press, on a phone with no keyboard: not drawn.
                className="max-md:hidden"
              >
                {shortcut}
              </Kbd>
            ) : null}
            {menu ? (
              <ChevronDownIcon
                size="sm"
                className={cn('text-fg-ghost', chevronFlip)}
              />
            ) : null}
          </>
        ),
      },
      props,
    ),
  });
}

Button.displayName = 'Button';
