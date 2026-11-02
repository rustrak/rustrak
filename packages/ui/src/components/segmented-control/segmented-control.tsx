'use client';

import { Radio } from '@base-ui/react/radio';
import { RadioGroup } from '@base-ui/react/radio-group';
import { type ReactNode, useLayoutEffect, useRef, useState } from 'react';
import { focusRing } from '../../lib/focus';
import { interactiveTransition, slideTransition } from '../../lib/motion';
import { tv } from '../../lib/tv';

/**
 * A short, closed set of mutually exclusive choices, all visible at once: the
 * time range (1 h · 24 h · 7 d · 14 d · 30 d · All), sort direction, a chart's
 * unit.
 *
 * It is not a dropdown and it is not tabs. A dropdown would hide five options
 * to save the width of four words, and this control is used constantly; tabs
 * change what the page is about, whereas this changes a parameter of what is
 * already on it.
 *
 * Beyond about six options this is the wrong control -- the row starts
 * competing with the page title for width. That is where a menu belongs.
 *
 * ## One option is always chosen, and that is why it is a radio group
 *
 * It used to be a toggle group, and clicking the selected option switched it
 * off: a segmented control showing "1 h · 24 h · 7 d" with none of them lit,
 * and a chart with no range. There is no such state. A set of choices where
 * exactly one is always taken *is* a radio group, so this is built on one --
 * which means the browser refuses to deselect, arrow keys move through the
 * options, and screen readers announce "1 of 6" instead of six unrelated
 * toggle buttons. Nothing had to be written to prevent the empty state; the
 * right primitive does not have it.
 *
 * ## The chip slides
 *
 * The selected background travels from the old option to the new one instead
 * of blinking out and in somewhere else. The travel is what says these are the
 * same six options and only the choice moved; a chip that reappears elsewhere
 * reads as the row itself having changed. It is measured off the DOM rather
 * than computed, because the labels are free text and their widths are not
 * known in advance.
 */
const segmented = tv({
  slots: {
    root: [
      'relative inline-flex h-control-md shrink-0 items-center gap-0.5',
      'rounded-lg bg-surface-active p-0.5',
      'has-data-disabled:opacity-60',
    ],
    /*
     * The chip sits *behind* the labels rather than being their background, and
     * that is the whole trick: one element that moves, instead of a background
     * appearing on one child and disappearing from another.
     */
    chip: [
      'pointer-events-none absolute top-0.5 bottom-0.5 left-0 rounded-md',
      'bg-surface-floating shadow-raised',
      slideTransition,
    ],
    item: [
      'relative z-10 flex h-control-xs shrink-0 items-center justify-center rounded-md px-3',
      'text-meta whitespace-nowrap text-fg-subtle',
      'hover:text-fg-secondary',
      'data-checked:font-medium data-checked:text-fg',
      'data-disabled:text-fg-disabled data-disabled:hover:text-fg-disabled',
      interactiveTransition,
      focusRing,
    ],
  },
});

const styles = segmented();

export interface SegmentedControlProps {
  children: ReactNode;
  /** The chosen option. Controlled; pair it with `onValueChange`. */
  value?: string;
  /** The option chosen on first render. Uncontrolled. */
  defaultValue?: string;
  onValueChange?: (value: string) => void;
  disabled?: boolean;
  /** What is being chosen. Without it the group has no accessible name. */
  'aria-label': string;
  className?: string;
}

export function SegmentedControl({
  children,
  value,
  defaultValue,
  onValueChange,
  disabled,
  className,
  'aria-label': ariaLabel,
}: SegmentedControlProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  /*
   * The chosen option, mirrored out of Base UI.
   *
   * Uncontrolled, the click never reaches this component: Base UI flips
   * `data-checked` inside itself, nothing here re-renders, and the measuring
   * effect does not run. The chip still moved, but only because the newly
   * checked item gained `data-checked:font-semibold` and both items therefore
   * resized, which woke the `ResizeObserver`. Drop the weight, or give the
   * items a fixed width, and the chip quietly stops following the selection
   * while every test that measures widths keeps passing.
   */
  const [uncontrolled, setUncontrolled] = useState(defaultValue);
  const selected = value ?? uncontrolled;
  const [chip, setChip] = useState<{ left: number; width: number } | null>(
    null,
  );

  /*
   * `useLayoutEffect`, not `useEffect`: it places the chip before the browser
   * paints, so on the first frame it is already under the chosen option
   * instead of being seen travelling in from the left edge.
   */
  useLayoutEffect(() => {
    const root = rootRef.current;

    if (root == null) {
      return;
    }

    const measure = () => {
      const checked = root.querySelector<HTMLElement>('[data-checked]');

      setChip(
        checked == null
          ? null
          : { left: checked.offsetLeft, width: checked.offsetWidth },
      );
    };

    measure();

    /*
     * Labels move when the window is resized and, more often, when the webfont
     * finishes loading and every word gets slightly wider. Without this the
     * chip stays where the fallback face put it.
     */
    const observer = new ResizeObserver(measure);
    observer.observe(root);
    for (const child of root.children) {
      observer.observe(child);
    }

    return () => observer.disconnect();
  }, [selected, children]);

  return (
    <RadioGroup
      ref={rootRef}
      value={value}
      defaultValue={defaultValue}
      onValueChange={(next) => {
        setUncontrolled(String(next));
        onValueChange?.(String(next));
      }}
      disabled={disabled}
      aria-label={ariaLabel}
      className={styles.root({ className })}
    >
      {/* Only once measured. Rendered from the start it would mount at zero and
          slide across on the first paint, which is the thing being avoided. */}
      {chip == null ? null : (
        <span
          aria-hidden="true"
          className={styles.chip()}
          style={{ width: chip.width, translate: `${chip.left}px` }}
        />
      )}

      {children}
    </RadioGroup>
  );
}

SegmentedControl.displayName = 'SegmentedControl';

export interface SegmentedItemProps {
  /** What this option means to the caller. */
  value: string;
  children: ReactNode;
  disabled?: boolean;
  className?: string;
}

export function SegmentedItem({
  value,
  children,
  disabled,
  className,
}: SegmentedItemProps) {
  return (
    <Radio.Root
      value={value}
      disabled={disabled}
      className={styles.item({ className })}
    >
      {children}
    </Radio.Root>
  );
}

SegmentedItem.displayName = 'SegmentedItem';
