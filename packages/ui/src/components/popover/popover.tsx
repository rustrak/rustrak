import { Popover as BasePopover } from '@base-ui/react/popover';
import type { ReactElement, ReactNode } from 'react';
import { PANEL_OFFSET, panelSurface } from '../../lib/panel';
import { tv } from '../../lib/tv';

/**
 * A floating panel with interactive content: a filter with its search field, a
 * date range with its presets.
 *
 * It exists beside `Menu` because the two manage focus differently, and the
 * difference is the whole point. A menu owns focus -- arrow keys walk items,
 * typing selects by first letter -- which is exactly wrong for a panel whose
 * first element is an input. A popover hands focus to its content and lets it
 * be content.
 *
 * The surface is the menu's (`lib/panel.ts`).
 */
const popover = tv({
  slots: {
    // See the note on Menu: the stacking level must ride on the positioner,
    // because Base UI's `position: fixed` opens a stacking context there.
    positioner: 'z-50',
    popup: [
      'flex max-h-(--available-height) min-w-52 flex-col',
      'overflow-x-hidden overflow-y-auto outline-none',
      panelSurface,
    ],
    /*
     * The header names what the panel filters: "Nivel", "Eventos". It reads as
     * a column heading rather than a title because that is what it is -- the
     * column, restated where the cursor now is.
     */
    title: [
      'border-border-subtle border-b px-3 pt-2.5 pb-2',
      'text-column text-fg-meta',
    ],
    /*
     * The keyboard footer, same as the menu's: every panel in the product ends
     * with one, because its users live in the keyboard.
     */
    hints: [
      'flex shrink-0 gap-3.5 border-border-subtle border-t',
      'px-3 pt-2 pb-2 font-mono text-mono-sm text-fg-ghost',
    ],
  },
});

const styles = popover();

export interface PopoverProps {
  /** What opens it. Must accept props and a ref: a `Button`, for example. */
  trigger: ReactElement;
  children: ReactNode;
  /** The heading that names the subject. Omitted, the content starts at once. */
  title?: string;
  /** The keyboard footer: `['↑↓ move', 'Enter applies', 'Esc closes']`. */
  hints?: string[];
  side?: BasePopover.Positioner.Props['side'];
  align?: BasePopover.Positioner.Props['align'];
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  /**
   * The panel's measurements, for content that needs more than the minimum.
   * Size and padding only; the surface comes from the recipe.
   */
  popupClassName?: string;
  /**
   * Where focus lands when the panel opens. Base UI's default is the popup
   * itself; a filter panel points this at its search input instead, so the
   * panel opens ready to be typed into.
   */
  initialFocus?: BasePopover.Popup.Props['initialFocus'];
}

export function Popover({
  trigger,
  children,
  title,
  hints,
  side = 'bottom',
  align = 'start',
  open,
  onOpenChange,
  popupClassName,
  initialFocus,
}: PopoverProps) {
  return (
    <BasePopover.Root open={open} onOpenChange={onOpenChange}>
      <BasePopover.Trigger render={trigger} />
      <BasePopover.Portal>
        <BasePopover.Positioner
          side={side}
          align={align}
          sideOffset={PANEL_OFFSET}
          className={styles.positioner()}
        >
          <BasePopover.Popup
            initialFocus={initialFocus}
            className={styles.popup({ className: popupClassName })}
          >
            {title ? (
              <BasePopover.Title className={styles.title()}>
                {title}
              </BasePopover.Title>
            ) : null}
            {children}
            {hints?.length ? (
              <span aria-hidden="true" className={styles.hints()}>
                {hints.map((hint) => (
                  <span key={hint}>{hint}</span>
                ))}
              </span>
            ) : null}
          </BasePopover.Popup>
        </BasePopover.Positioner>
      </BasePopover.Portal>
    </BasePopover.Root>
  );
}

Popover.displayName = 'Popover';
