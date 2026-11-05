import { Field as BaseField } from '@base-ui/react/field';
import { cn } from '../../lib/cn';
import type { WithClassName } from '../../lib/types';

/**
 * A form field: label, control, hint and error.
 *
 * Base UI's `Field.Root` is what ties the three together by `id` and
 * `aria-describedby`, and what hands `data-invalid` and `data-disabled` down.
 * This file only adds the look and the rules the design settled:
 *
 *   - the label wraps to two lines and never truncates: half a label says
 *     nothing;
 *   - the required asterisk is decoration -- what announces it is the
 *     control's own `required`;
 *   - the error replaces the hint while it stands, so a field never stacks
 *     two lines of small print.
 */
export interface FieldProps extends WithClassName<BaseField.Root.Props> {}

export function Field({ className, ...props }: FieldProps) {
  return (
    <BaseField.Root
      className={cn('flex min-w-0 flex-col gap-1.5', className)}
      {...props}
    />
  );
}

Field.displayName = 'Field';

export interface FieldLabelProps extends WithClassName<BaseField.Label.Props> {
  /** Paints the asterisk. The control must carry `required` too. */
  required?: boolean;
  /**
   * The label keeps existing for the screen reader but is not drawn.
   *
   * For a control whose name is already said nearby -- a column heading, a
   * dialog title -- repeating it is noise, but a control with no accessible
   * name cannot be filled without seeing the screen.
   */
  srOnly?: boolean;
}

export function FieldLabel({
  required,
  srOnly,
  className,
  children,
  ...props
}: FieldLabelProps) {
  if (srOnly) {
    return (
      <BaseField.Label className={cn('sr-only', className)} {...props}>
        {children}
      </BaseField.Label>
    );
  }

  return (
    <BaseField.Label
      className={cn(
        'w-fit text-fg-secondary text-label data-disabled:text-fg-disabled',
        className,
      )}
      {...props}
    >
      {children}
      {required ? (
        <span aria-hidden="true" className="text-danger-fg">
          {' *'}
        </span>
      ) : null}
    </BaseField.Label>
  );
}

FieldLabel.displayName = 'FieldLabel';

export interface FieldHintProps
  extends WithClassName<BaseField.Description.Props> {}

/**
 * The hint below the field: where the value goes, what format it takes.
 *
 * Unmounted while the field is invalid: `FieldError` takes its place, and two
 * lines of small print under one field is a wall, not guidance. Hiding it
 * with CSS alone would leave its id registered in `aria-describedby` -- the
 * screen reader would still read a hint the eye can't see -- so this reads
 * validity through `Field.Validity` instead of rendering `Description`
 * unconditionally.
 */
export function FieldHint({ className, ...props }: FieldHintProps) {
  return (
    <BaseField.Validity>
      {({ validity }) =>
        validity.valid === false ? null : (
          <BaseField.Description
            className={cn('text-fg-subtle text-hint', className)}
            {...props}
          />
        )
      }
    </BaseField.Validity>
  );
}

FieldHint.displayName = 'FieldHint';

export interface FieldErrorProps extends WithClassName<BaseField.Error.Props> {}

/**
 * The validation error. It replaces the hint while it stands: two lines of
 * small print under every field of a long form is a wall, not guidance.
 */
export function FieldError({ className, ...props }: FieldErrorProps) {
  return (
    <BaseField.Error
      className={cn('text-danger-fg text-hint', className)}
      {...props}
    />
  );
}

FieldError.displayName = 'FieldError';
