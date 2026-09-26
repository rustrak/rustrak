import { type FieldValues, type Path, useFormContext } from 'react-hook-form';
import {
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/shared/ui/components/shadcn/form';
import { Input } from '@/shared/ui/components/shadcn/input';

/**
 * One labelled text input of a provider form: a URL, a secret, a token.
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
  disabled,
}: {
  name: Path<T>;
  label: string;
  placeholder: string;
  description?: string;
  type?: 'text' | 'url' | 'password' | 'email';
  disabled: boolean;
}) {
  const { control } = useFormContext<T>();

  return (
    <FormField
      control={control}
      name={name}
      render={({ field }) => (
        <FormItem>
          <FormLabel className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
            {label}
          </FormLabel>
          <FormControl>
            <Input
              type={type}
              placeholder={placeholder}
              disabled={disabled}
              {...field}
            />
          </FormControl>
          {description ? (
            <FormDescription>{description}</FormDescription>
          ) : null}
          <FormMessage />
        </FormItem>
      )}
    />
  );
}
