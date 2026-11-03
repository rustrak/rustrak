import { Input } from '@rustrak/ui';
import { type FieldValues, type Path, useFormContext } from 'react-hook-form';
import { useTranslations } from 'use-intl';
import {
  FormControl,
  FormField,
  FormItem,
  FormMessage,
} from '@/shared/ui/components/form';
import { FieldRow } from './form-chrome';

/**
 * What the integration is called, which every provider asks for identically.
 *
 * Generic over the form shape rather than tied to one: the three schemas share
 * only `name` and `is_enabled`, and a union of them would let a caller reach
 * for a field its own provider does not have.
 */
export function NameField<T extends FieldValues & { name: string }>({
  placeholder,
  disabled,
}: {
  placeholder: string;
  disabled: boolean;
}) {
  const t = useTranslations('alerts');
  const { control } = useFormContext<T>();

  return (
    <FormField
      control={control}
      name={'name' as Path<T>}
      render={({ field }) => (
        <FormItem>
          <FieldRow
            title={t('common.name')}
            description={t('page.nameDescription')}
          >
            <FormControl>
              <Input placeholder={placeholder} disabled={disabled} {...field} />
            </FormControl>
            <FormMessage className="mt-1.5" />
          </FieldRow>
        </FormItem>
      )}
    />
  );
}
