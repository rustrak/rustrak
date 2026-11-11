import { Input } from '@rustrak/ui';
import {
  useFieldContext,
  useFieldInvalid,
} from '@/shared/ui/components/app-form';
import { FieldRow } from './form-chrome';

/**
 * One text setting of a provider form: a URL, a secret, a token. The
 * explanation sits beside the input, where a settings row keeps it.
 * Rendered inside the `form.AppField` it edits.
 */
export function TextInputField({
  label,
  placeholder,
  description,
  type = 'text',
  numeric,
  autoComplete,
  disabled,
}: {
  label: string;
  placeholder: string;
  description?: string;
  type?: 'text' | 'url' | 'password' | 'email';
  /** Stored as a number: a port. */
  numeric?: boolean;
  autoComplete?: string;
  disabled: boolean;
}) {
  const field = useFieldContext<string | number | undefined>();
  const invalid = useFieldInvalid();

  return (
    <FieldRow title={label} description={description}>
      {(id) => (
        <Input
          id={id}
          name={field.name}
          type={numeric ? 'number' : type}
          placeholder={placeholder}
          // A secret is not something to offer from the browser's
          // saved logins, and filling one in by accident replaces it.
          autoComplete={
            autoComplete ?? (type === 'password' ? 'new-password' : 'off')
          }
          disabled={disabled}
          invalid={invalid}
          value={field.state.value ?? ''}
          onBlur={field.handleBlur}
          onChange={(e) => {
            if (!numeric) return field.handleChange(e.target.value);
            // Empty stays empty -- the schema says it is required --
            // rather than snapping to a default while the reader types.
            const n = Number.parseInt(e.target.value, 10);
            field.handleChange(Number.isNaN(n) ? '' : n);
          }}
        />
      )}
    </FieldRow>
  );
}
