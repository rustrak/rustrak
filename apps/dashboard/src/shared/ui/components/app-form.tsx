/**
 * TanStack Form, pre-bound to `@rustrak/ui`'s field anatomy. Every form in the
 * dashboard is `useAppForm` with a Zod schema on `validators.onDynamic`: the
 * schema runs on submit, then on every change once a submit was attempted.
 */

import {
  cn,
  Field,
  FieldError,
  FieldHint,
  FieldLabel,
  Input,
  type InputProps,
} from '@rustrak/ui';
import {
  createFormHook,
  createFormHookContexts,
  revalidateLogic,
  useSelector,
} from '@tanstack/react-form';
import type { ComponentProps, ReactNode } from 'react';
import { focusFirstInvalid, serverFormError } from '@/shared/lib/form-errors';
import { PasswordInput } from '@/shared/ui/components/password-input';

export const { fieldContext, formContext, useFieldContext, useFormContext } =
  createFormHookContexts();

export { revalidateLogic };

/** The first message on a field: a schema issue or a server string. */
export function fieldErrorText(errors: readonly unknown[]): string | undefined {
  for (const error of errors) {
    if (typeof error === 'string' && error) return error;
    if (error && typeof error === 'object' && 'message' in error) {
      return String(error.message);
    }
  }
  return undefined;
}

/** Whether the bound field currently shows an error. */
export function useFieldInvalid(): boolean {
  const field = useFieldContext<unknown>();
  return fieldErrorText(field.state.meta.errors) !== undefined;
}

interface ItemProps {
  label?: ReactNode;
  hint?: ReactNode;
  required?: boolean;
  srOnlyLabel?: boolean;
  className?: string;
  children?: ReactNode;
}

/** Label, any control, hint and the field's error. */
function Item({
  label,
  hint,
  required,
  srOnlyLabel,
  className,
  children,
}: ItemProps) {
  const field = useFieldContext<unknown>();
  const error = fieldErrorText(field.state.meta.errors);

  return (
    <Field name={field.name} invalid={!!error} className={className}>
      {label ? (
        <FieldLabel required={required} srOnly={srOnlyLabel}>
          {label}
        </FieldLabel>
      ) : null}
      {children}
      {hint ? <FieldHint>{hint}</FieldHint> : null}
      {error ? <FieldError match>{error}</FieldError> : null}
    </Field>
  );
}

type BoundInputProps = Omit<
  InputProps,
  'value' | 'onChange' | 'onBlur' | 'name' | 'invalid'
> &
  Omit<ItemProps, 'children'>;

function TextField({
  label,
  hint,
  required,
  srOnlyLabel,
  className,
  ...props
}: BoundInputProps) {
  const field = useFieldContext<string>();
  const invalid = fieldErrorText(field.state.meta.errors) !== undefined;

  return (
    <Item
      label={label}
      hint={hint}
      required={required}
      srOnlyLabel={srOnlyLabel}
      className={className}
    >
      <Input
        {...props}
        name={field.name}
        value={field.state.value ?? ''}
        onChange={(event) => field.handleChange(event.target.value)}
        onBlur={field.handleBlur}
        invalid={invalid}
      />
    </Item>
  );
}

function PasswordField({
  label,
  hint,
  required,
  srOnlyLabel,
  className,
  ...props
}: Omit<ComponentProps<typeof PasswordInput>, 'value' | 'onChange'> &
  Omit<ItemProps, 'children'>) {
  const field = useFieldContext<string>();
  const invalid = fieldErrorText(field.state.meta.errors) !== undefined;

  return (
    <Item
      label={label}
      hint={hint}
      required={required}
      srOnlyLabel={srOnlyLabel}
      className={className}
    >
      <PasswordInput
        {...props}
        name={field.name}
        value={field.state.value ?? ''}
        onChange={(event) => field.handleChange(event.target.value)}
        onBlur={field.handleBlur}
        invalid={invalid}
      />
    </Item>
  );
}

/**
 * `<form noValidate>` that hands submit to TanStack Form, then focuses the
 * first invalid control so the reader lands on what needs fixing.
 */
function Form({
  onSubmit: _ignored,
  ...props
}: Omit<ComponentProps<'form'>, 'noValidate'>) {
  const form = useFormContext();
  return (
    <form
      noValidate
      onSubmit={async (event) => {
        event.preventDefault();
        event.stopPropagation();
        const element = event.currentTarget;
        await form.handleSubmit();
        if (!form.state.isValid) focusFirstInvalid(element);
      }}
      {...props}
    />
  );
}

/**
 * Where a server failure lands when it named no input on screen.
 * `role="alert"`: it appears after a submit, so a screen reader is told.
 */
function RootError({ className }: { className?: string }) {
  const form = useFormContext();
  const message = useSelector(form.store, (state) =>
    serverFormError(state.errorMap),
  );

  if (!message) return null;

  return (
    <p role="alert" className={cn('text-danger-fg text-hint', className)}>
      {message}
    </p>
  );
}

export const { useAppForm, withForm } = createFormHook({
  fieldContext,
  formContext,
  fieldComponents: { Item, TextField, PasswordField },
  formComponents: { Form, RootError },
});
