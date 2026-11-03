import { Field as BaseField } from '@base-ui/react/field';
import {
  type ComponentPropsWithoutRef,
  type ComponentPropsWithRef,
  cloneElement,
  isValidElement,
  type ReactElement,
  type ReactNode,
} from 'react';
import { cn } from '../../lib/cn';
import { focusRing } from '../../lib/focus';
import { interactiveTransition, pressScaleSmall } from '../../lib/motion';
import type { VariantProps } from '../../lib/tv';
import type { IconComponent } from '../icon/icon';
import { inputShell } from './input-shell';

export interface InputProps
  extends Omit<ComponentPropsWithRef<'input'>, 'size'>,
    Omit<VariantProps<typeof inputShell>, 'multiline'> {
  /** A symbol or icon glued to the start: a search glass, a unit. */
  leading?: ReactNode;
  /** The button at the end: clear, reveal, open. Use `InputAction`. */
  action?: ReactNode;
  /** Marks the field invalid without leaning on native validation. */
  invalid?: boolean;
  /** Classes for the box; `className` goes to the `input` itself. */
  boxClassName?: string;
}

/**
 * The trailing action rides on the control's own `disabled`: Base UI's
 * `Field.Root` and a bare `disabled` prop both reach `BaseField.Control` the
 * same way, but `action` is a sibling the control knows nothing about, so
 * without this it stays focusable and clickable on a disabled field.
 */
function disabledAction(action: ReactNode, disabled: boolean | undefined) {
  if (!disabled || !isValidElement(action)) return action;
  return cloneElement(action as ReactElement<{ disabled?: boolean }>, {
    disabled: true,
  });
}

/**
 * The text field. Inside a `Field` it wires itself to the label and the
 * error through Base UI's control; on its own it needs an `aria-label`.
 */
export function Input({
  size,
  numeric,
  leading,
  action,
  invalid,
  disabled,
  className,
  boxClassName,
  ...props
}: InputProps) {
  // Variants are pulled off one by one, never with a rest: whatever stays in
  // `...props` lands on the `<input>`, and a variant that slips through
  // paints nothing and comes back out of React as an unknown DOM attribute.
  const styles = inputShell({ size, numeric });

  return (
    <span className={styles.box({ className: boxClassName })}>
      {leading ? (
        <span aria-hidden="true" className={styles.adornment()}>
          {leading}
        </span>
      ) : null}

      <BaseField.Control
        disabled={disabled}
        aria-invalid={invalid || undefined}
        data-invalid={invalid || undefined}
        className={styles.control({ className })}
        {...props}
      />

      {disabledAction(action, disabled)}
    </span>
  );
}

Input.displayName = 'Input';

export interface TextareaProps
  extends ComponentPropsWithoutRef<'textarea'>,
    Pick<VariantProps<typeof inputShell>, 'size'> {
  invalid?: boolean;
  boxClassName?: string;
}

/** The multi-line field, in the same box: it grows, the frame stays one. */
export function Textarea({
  size,
  invalid,
  className,
  boxClassName,
  rows = 3,
  ...props
}: TextareaProps) {
  const styles = inputShell({ size, multiline: true });

  return (
    <span className={styles.box({ className: boxClassName })}>
      {/* The textarea's own props ride the rendered element: `Control` is
          typed for an input, and only its wiring belongs to it. */}
      <BaseField.Control
        render={
          <textarea
            rows={rows}
            aria-invalid={invalid || undefined}
            data-invalid={invalid || undefined}
            className={styles.control({ className })}
            {...props}
          />
        }
      />
    </span>
  );
}

Textarea.displayName = 'Textarea';

export interface InputActionProps
  extends Omit<ComponentPropsWithRef<'button'>, 'children'> {
  icon: IconComponent;
  /** What the button does. Required: it carries no visible text. */
  'aria-label': string;
}

/**
 * The button at the field's end: the clear cross of a search, the reveal
 * eye of a secret, the chevron of a closed list.
 */
export function InputAction({
  icon: Icon,
  className,
  ...props
}: InputActionProps) {
  return (
    <button
      type="button"
      className={cn(
        'flex size-6 shrink-0 items-center justify-center rounded-sm',
        'text-fg-ghost',
        'hover:bg-surface-selected hover:text-fg',
        'disabled:text-fg-disabled',
        interactiveTransition,
        pressScaleSmall,
        focusRing,
        className,
      )}
      {...props}
    >
      <Icon size="md" aria-hidden="true" />
    </button>
  );
}

InputAction.displayName = 'InputAction';
