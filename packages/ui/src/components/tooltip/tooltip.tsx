import { Tooltip as BaseTooltip } from '@base-ui/react/tooltip';
import type { ReactElement, ReactNode } from 'react';
import { popTransition } from '../../lib/motion';
import { tv } from '../../lib/tv';

const popup = tv({
  base: [
    'flex min-h-7 max-w-64 items-center gap-2 rounded-md px-2.5 py-1',
    'border border-border-tooltip bg-surface-tooltip',
    'text-control text-fg-on-tooltip shadow-tooltip',
    popTransition,
  ],
});

/* The stacking level goes on the positioner, not on the popup: the positioner
   is what opens the stacking context. See `menu/menu.tsx`. */
const positioner = 'z-50';

/**
 * Shares the delay between every tooltip in an area: once one is open the next
 * appears instantly. Without it, running along a rail of twelve icons means
 * waiting twelve times.
 *
 * Goes once, at the root of the application. `AppShell` already includes it.
 */
export const TooltipProvider = BaseTooltip.Provider;

export interface TooltipProps {
  /** What it explains. One line; if it needs two, it is not a tooltip. */
  content: ReactNode;
  /** The element that opens it. Must accept props and a ref. */
  children: ReactElement;
  side?: BaseTooltip.Positioner.Props['side'];
  /** Milliseconds before it appears. */
  delay?: number;
  /** Turns it off without taking it out of the tree. */
  disabled?: boolean;
  /**
   * Anything else falls through to the trigger.
   *
   * That is what lets a tooltip wrap something which is already the trigger of
   * something else -- a collapsed sidebar item that also opens a flyout. The
   * wrapper hands its props to this component and this component drops them on
   * the child. Without it you would have to choose between the menu and the
   * explanation.
   */
  [prop: string]: unknown;
}

/**
 * The explanation on hover.
 *
 * Mandatory on icon-only buttons and on every item in the collapsed sidebar.
 * It never carries information that is not somewhere else: it cannot be read
 * with a keyboard on a phone, and not everyone uses a pointer.
 */
export function Tooltip({
  content,
  children,
  side = 'top',
  delay,
  disabled,
  ...rest
}: TooltipProps) {
  // Disabled stays in the tree: unwrapping the child would remount it, and a
  // collapsing sidebar row would lose its transition mid-flight.
  return (
    <BaseTooltip.Root disabled={disabled}>
      {/* `rest` goes to Base UI's trigger, which merges it into the child. */}
      <BaseTooltip.Trigger delay={delay} render={children} {...rest} />
      <BaseTooltip.Portal>
        <BaseTooltip.Positioner
          side={side}
          sideOffset={8}
          className={positioner}
        >
          <BaseTooltip.Popup className={popup()}>{content}</BaseTooltip.Popup>
        </BaseTooltip.Positioner>
      </BaseTooltip.Portal>
    </BaseTooltip.Root>
  );
}

Tooltip.displayName = 'Tooltip';
