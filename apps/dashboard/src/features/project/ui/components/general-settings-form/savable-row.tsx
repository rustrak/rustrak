import { Input, Spinner } from '@rustrak/ui';
import type { Path, UseFormReturn } from 'react-hook-form';
import {
  FormControl,
  FormField,
  FormItem,
  FormMessage,
} from '@/shared/ui/components/form';
import { SettingRow } from '@/shared/ui/components/setting-row';
import type { GeneralSettingsFormData } from './general-settings-form';

/**
 * One settings field that saves on its own.
 *
 * Each row commits independently rather than the page having a single Save,
 * so a user who renamed the project and then thought better of the slug is
 * not forced to submit both or neither.
 *
 * The button stays disabled until the value actually differs from what is
 * stored: a Save that does nothing still costs a round trip and a toast
 * claiming something changed.
 */
export function SavableRow({
  form,
  name,
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
  form: UseFormReturn<GeneralSettingsFormData>;
  name: Path<GeneralSettingsFormData>;
  title: string;
  description: string;
  placeholder: string;
  className?: string;
  isPending: boolean;
  hasChanges: boolean;
  onSave: () => void;
  onEdit: () => void;
}) {
  const id = `project-${name}`;

  return (
    <SettingRow title={title} description={description} htmlFor={id}>
      <FormField
        control={form.control}
        name={name}
        render={({ field }) => (
          <FormItem>
            <FormControl>
              <Input
                id={id}
                placeholder={placeholder}
                className={className}
                disabled={isPending || readOnly}
                aria-busy={isPending || undefined}
                leading={
                  isPending ? (
                    <Spinner size="sm" aria-hidden="true" />
                  ) : undefined
                }
                {...field}
                onChange={(event) => {
                  field.onChange(event);
                  onEdit();
                }}
                // Saved when the reader is done with it: on Enter, or on
                // leaving the field. A per-row Save button asked them to
                // confirm what they had just typed.
                onBlur={() => {
                  field.onBlur();
                  if (hasChanges && field.value?.trim()) onSave();
                }}
                onKeyDown={(event) => {
                  // The Enter that confirms an IME candidate is not a save.
                  if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
                    event.preventDefault();
                    if (hasChanges && field.value?.trim()) onSave();
                  }
                }}
              />
            </FormControl>
            <FormMessage />
          </FormItem>
        )}
      />
    </SettingRow>
  );
}
