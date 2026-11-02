import { Input } from '@rustrak/ui';
import { type FieldValues, type Path, useFormContext } from 'react-hook-form';
import {
  FormControl,
  FormField,
  FormItem,
  FormMessage,
} from '@/shared/ui/components/form';
import { FieldRow } from './form-chrome';

/**
 * One text setting of a provider form: a URL, a secret, a token. The
 * explanation sits beside the input, where a settings row keeps it.
 *
 * Generic over the form shape like `NameField`, so `name` is checked against
 * the caller's own schema.
 */
export function TextInputField<T extends FieldValues>({
  name,
  label,
  placeholder,
  description,
  type = 'text',
  numeric,
  autoComplete,
  disabled,
}: {
  name: Path<T>;
  label: string;
  placeholder: string;
  description?: string;
  type?: 'text' | 'url' | 'password' | 'email';
  /** Stored as a number: a port. */
  numeric?: boolean;
  autoComplete?: string;
  disabled: boolean;
}) {
  const { control } = useFormContext<T>();

  return (
    <FormField
      control={control}
      name={name}
      render={({ field }) => (
        <FormItem>
          <FieldRow title={label} description={description}>
            <FormControl>
              <Input
                type={numeric ? 'number' : type}
                placeholder={placeholder}
                // A secret is not something to offer from the browser's
                // saved logins, and filling one in by accident replaces it.
                autoComplete={
                  autoComplete ?? (type === 'password' ? 'new-password' : 'off')
                }
                disabled={disabled}
                {...field}
                onChange={
                  numeric
                    ? (e) => {
                        // Empty stays empty -- the schema says it is
                        // required -- rather than snapping to a default
                        // while the reader is still typing.
                        const n = Number.parseInt(e.target.value, 10);
                        field.onChange(Number.isNaN(n) ? '' : n);
                      }
                    : field.onChange
                }
              />
            </FormControl>
            <FormMessage className="mt-1.5" />
          </FieldRow>
        </FormItem>
      )}
    />
  );
}
