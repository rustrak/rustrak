import { Input } from '@rustrak/ui';
import { useTranslations } from 'use-intl';
import {
  useFieldContext,
  useFieldInvalid,
} from '@/shared/ui/components/app-form';
import { FieldRow } from './form-chrome';

/** What the integration is called. Rendered inside `form.AppField name="name"`. */
export function NameField({
  placeholder,
  disabled,
}: {
  placeholder: string;
  disabled: boolean;
}) {
  const t = useTranslations('alerts');
  const field = useFieldContext<string>();
  const invalid = useFieldInvalid();

  return (
    <FieldRow title={t('common.name')} description={t('page.nameDescription')}>
      {(id) => (
        <Input
          id={id}
          name={field.name}
          placeholder={placeholder}
          disabled={disabled}
          invalid={invalid}
          value={field.state.value}
          onChange={(e) => field.handleChange(e.target.value)}
          onBlur={field.handleBlur}
        />
      )}
    </FieldRow>
  );
}
