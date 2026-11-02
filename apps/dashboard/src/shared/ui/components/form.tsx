/**
 * react-hook-form, wired to the design system's field anatomy: a label, the
 * control, a hint, and the error that replaces the hint. The glue (ids,
 * `aria-describedby`, `aria-invalid`) is react-hook-form's; every class is a
 * token from `@rustrak/ui`, so a form reads like `Field` does in Storybook.
 */

import { cn } from '@rustrak/ui';
import * as React from 'react';
import {
  Controller,
  type ControllerProps,
  type FieldPath,
  type FieldValues,
  FormProvider,
  useFormContext,
  useFormState,
} from 'react-hook-form';

const Form = FormProvider;

type FormFieldContextValue<
  TFieldValues extends FieldValues = FieldValues,
  TName extends FieldPath<TFieldValues> = FieldPath<TFieldValues>,
> = {
  name: TName;
};

const FormFieldContext = React.createContext<FormFieldContextValue>(
  {} as FormFieldContextValue,
);

const FormField = <
  TFieldValues extends FieldValues = FieldValues,
  TName extends FieldPath<TFieldValues> = FieldPath<TFieldValues>,
>({
  ...props
}: ControllerProps<TFieldValues, TName>) => {
  return (
    <FormFieldContext.Provider value={{ name: props.name }}>
      <Controller {...props} />
    </FormFieldContext.Provider>
  );
};

const useFormField = () => {
  const fieldContext = React.useContext(FormFieldContext);
  const itemContext = React.useContext(FormItemContext);
  const { getFieldState } = useFormContext();
  const formState = useFormState({ name: fieldContext.name });
  const fieldState = getFieldState(fieldContext.name, formState);

  if (!fieldContext) {
    throw new Error('useFormField should be used within <FormField>');
  }

  const { id } = itemContext;

  return {
    id,
    name: fieldContext.name,
    formItemId: `${id}-form-item`,
    formDescriptionId: `${id}-form-item-description`,
    formMessageId: `${id}-form-item-message`,
    ...fieldState,
  };
};

type FormItemContextValue = {
  id: string;
};

const FormItemContext = React.createContext<FormItemContextValue>(
  {} as FormItemContextValue,
);

function FormItem({ className, ...props }: React.ComponentProps<'div'>) {
  const id = React.useId();

  return (
    <FormItemContext.Provider value={{ id }}>
      <div
        data-slot="form-item"
        className={cn('flex min-w-0 flex-col gap-1.5', className)}
        {...props}
      />
    </FormItemContext.Provider>
  );
}

function FormLabel({ className, ...props }: React.ComponentProps<'label'>) {
  const { formItemId } = useFormField();

  return (
    <label
      data-slot="form-label"
      className={cn('text-label text-fg-secondary', className)}
      htmlFor={formItemId}
      {...props}
    />
  );
}

function FormControl({ children }: { children: React.ReactElement }) {
  const { error, formItemId, formDescriptionId, formMessageId } =
    useFormField();

  const child = children as React.ReactElement<Record<string, unknown>>;
  return React.cloneElement(child, {
    'data-slot': 'form-control',
    id: formItemId,
    'aria-describedby': !error
      ? `${formDescriptionId}`
      : `${formDescriptionId} ${formMessageId}`,
    'aria-invalid': !!error,
  });
}

function FormDescription({ className, ...props }: React.ComponentProps<'p'>) {
  const { formDescriptionId } = useFormField();

  return (
    <p
      data-slot="form-description"
      id={formDescriptionId}
      className={cn('text-hint text-fg-subtle', className)}
      {...props}
    />
  );
}

function FormMessage({ className, ...props }: React.ComponentProps<'p'>) {
  const { error, formMessageId } = useFormField();
  const body = error ? String(error?.message ?? '') : props.children;

  if (!body) {
    return null;
  }

  return (
    <p
      data-slot="form-message"
      id={formMessageId}
      className={cn('text-hint text-danger-fg', className)}
      {...props}
    >
      {body}
    </p>
  );
}

/**
 * The form-level slot `applyServerFieldErrors` writes to.
 *
 * Renders `root.serverError`, which is where a server failure lands when it
 * named no input, or named one this form does not register. Without this
 * component that branch of the helper would be invisible and the form would
 * simply appear to do nothing.
 *
 * `role="alert"` because it appears after a submit the user already committed
 * to, so a screen reader has to be told without being asked.
 */
function FormRootError({ className, ...props }: React.ComponentProps<'p'>) {
  const { errors } = useFormState();
  const message = errors.root?.serverError?.message;

  if (!message) {
    return null;
  }

  return (
    <p
      data-slot="form-root-error"
      role="alert"
      className={cn('text-hint text-danger-fg', className)}
      {...props}
    >
      {String(message)}
    </p>
  );
}

export {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  FormRootError,
  useFormField,
};
