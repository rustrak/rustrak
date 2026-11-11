import { Field, FieldError, Input, Spinner } from '@rustrak/ui';
import {
  fieldErrorText,
  useFieldContext,
} from '@/shared/ui/components/app-form';
import { SettingRow } from '@/shared/ui/components/setting-row';

/**
 * One settings field that saves on its own, rendered inside its
 * `form.AppField`.
 *
 * Each row commits independently rather than the page having a single Save,
 * so a user who renamed the project and then thought better of the slug is
 * not forced to submit both or neither.
 *
 * Nothing saves until the value actually differs from what is stored: a save
 * that does nothing still costs a round trip and a toast claiming something
 * changed.
 */
export function SavableRow({
  title,
  description,
  placeholder,
  className,
  isPending,
  readOnly = false,
  hasChanges,
  onSave,
  onEdit,
}: {
  /** Without `project:update`: shown, not changeable. */
  readOnly?: boolean;
  title: string;
  description: string;
  placeholder: string;
  className?: string;
  isPending: boolean;
  hasChanges: boolean;
  onSave: () => void;
  onEdit: () => void;
}) {
  const field = useFieldContext<string>();
  const id = `project-${field.name}`;
  const error = fieldErrorText(field.state.meta.errors);

  return (
    <SettingRow title={title} description={description} htmlFor={id}>
      <Field name={field.name} invalid={!!error}>
        <Input
          id={id}
          placeholder={placeholder}
          className={className}
          disabled={isPending || readOnly}
          aria-busy={isPending || undefined}
          leading={
            isPending ? <Spinner size="sm" aria-hidden="true" /> : undefined
          }
          name={field.name}
          value={field.state.value}
          invalid={!!error}
          onChange={(event) => {
            field.handleChange(event.target.value);
            onEdit();
          }}
          // Saved when the reader is done with it: on Enter, or on leaving
          // the field. A per-row Save button asked them to confirm what they
          // had just typed.
          // An emptied field still goes to `onSave`, whose schema says why
          // it cannot be saved.
          onBlur={() => {
            field.handleBlur();
            if (hasChanges) onSave();
          }}
          onKeyDown={(event) => {
            // The Enter that confirms an IME candidate is not a save. Enter
            // leaves the field and the blur saves: saving here too sent a
            // second request when the pending state disabled the input and
            // blurred it.
            if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
              event.preventDefault();
              event.currentTarget.blur();
            }
          }}
        />
        {error ? <FieldError match>{error}</FieldError> : null}
      </Field>
    </SettingRow>
  );
}
