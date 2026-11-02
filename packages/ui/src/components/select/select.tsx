import { Select as BaseSelect } from '@base-ui/react/select';
import type { ReactNode } from 'react';
import { cn } from '../../lib/cn';
import { focusRing } from '../../lib/focus';
import { interactiveTransition, popTransition } from '../../lib/motion';
import { tv } from '../../lib/tv';
import { SelectedIcon, SelectorIcon } from '../icon/icon-catalog';

/**
 * A closed list of choices, drawn like the text field it sits beside: same
 * box, same heights, same focus ring. The panel is the menu's panel, so a
 * select and a menu read as the same family.
 */
const select = tv({
  slots: {
    trigger: [
      'flex w-full min-w-0 items-center gap-2 rounded-md border ps-2.5 pe-2',
      'border-border-field bg-canvas text-start text-control text-fg',
      'hover:border-border-strong data-popup-open:border-border-strong',
      'data-disabled:cursor-not-allowed data-disabled:border-border-subtle',
      'data-disabled:bg-surface-disabled data-disabled:text-fg-disabled',
      'data-invalid:border-danger',
      interactiveTransition,
      focusRing,
    ],
    value: 'min-w-0 flex-1 truncate data-placeholder:text-fg-placeholder',
    icon: 'shrink-0 text-fg-subtle',
    positioner: 'z-50 outline-none',
    popup: [
      'max-h-(--available-height) min-w-(--anchor-width) overflow-y-auto',
      'rounded-xl border border-border bg-surface-floating p-1.5',
      'shadow-overlay outline-none',
      popTransition,
    ],
    item: [
      'flex h-menu-item cursor-default items-center gap-2.5 rounded-md',
      'ps-2.5 pe-2 text-control text-fg-muted outline-none select-none',
      'data-highlighted:bg-surface-selected data-highlighted:text-fg',
      'data-selected:text-fg data-disabled:text-fg-disabled',
    ],
    itemText: 'min-w-0 flex-1 truncate',
    indicator: 'shrink-0 text-fg-brand',
  },
  variants: {
    size: {
      md: { trigger: 'h-control-lg' },
      sm: { trigger: 'h-control-md' },
    },
  },
  defaultVariants: { size: 'md' },
});

export interface SelectProps<Value extends string> {
  value?: Value | null;
  defaultValue?: Value | null;
  onValueChange?: (value: Value) => void;
  /** Shown while nothing is chosen. */
  placeholder?: ReactNode;
  /**
   * What each value reads as in the closed box. Without it the box shows the
   * raw value, which is rarely what a person should read.
   */
  items?: Record<string, ReactNode>;
  size?: 'sm' | 'md';
  disabled?: boolean;
  invalid?: boolean;
  name?: string;
  id?: string;
  'aria-label'?: string;
  /** The hint or error that describes it, as a form field passes it. */
  'aria-describedby'?: string;
  className?: string;
  /** `SelectItem`s. */
  children: ReactNode;
}

export function Select<Value extends string>({
  value,
  defaultValue,
  onValueChange,
  placeholder,
  items,
  size,
  disabled,
  invalid,
  name,
  id,
  className,
  children,
  ...aria
}: SelectProps<Value>) {
  const styles = select({ size });

  return (
    <BaseSelect.Root
      value={value}
      defaultValue={defaultValue}
      onValueChange={(next) => {
        if (next != null) onValueChange?.(next as Value);
      }}
      items={items}
      disabled={disabled}
      name={name}
    >
      <BaseSelect.Trigger
        id={id}
        aria-label={aria['aria-label']}
        aria-describedby={aria['aria-describedby']}
        aria-invalid={invalid || undefined}
        data-invalid={invalid || undefined}
        className={styles.trigger({ className })}
      >
        <BaseSelect.Value
          placeholder={placeholder}
          className={styles.value()}
        />
        <BaseSelect.Icon className={styles.icon()}>
          <SelectorIcon size="md" />
        </BaseSelect.Icon>
      </BaseSelect.Trigger>
      <BaseSelect.Portal>
        <BaseSelect.Positioner
          className={styles.positioner()}
          sideOffset={4}
          alignItemWithTrigger={false}
        >
          <BaseSelect.Popup className={styles.popup()}>
            <BaseSelect.List>{children}</BaseSelect.List>
          </BaseSelect.Popup>
        </BaseSelect.Positioner>
      </BaseSelect.Portal>
    </BaseSelect.Root>
  );
}

Select.displayName = 'Select';

export interface SelectItemProps {
  value: string;
  disabled?: boolean;
  className?: string;
  children: ReactNode;
}

/** One choice. The check marks the chosen one; the highlight follows the pointer. */
export function SelectItem({
  value,
  disabled,
  className,
  children,
}: SelectItemProps) {
  const styles = select();

  return (
    <BaseSelect.Item
      value={value}
      disabled={disabled}
      className={cn(styles.item(), className)}
    >
      <BaseSelect.ItemText className={styles.itemText()}>
        {children}
      </BaseSelect.ItemText>
      <BaseSelect.ItemIndicator className={styles.indicator()}>
        <SelectedIcon size="md" />
      </BaseSelect.ItemIndicator>
    </BaseSelect.Item>
  );
}

SelectItem.displayName = 'SelectItem';
