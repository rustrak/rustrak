import { Switch } from '@rustrak/ui';
import { type FieldValues, type Path, useFormContext } from 'react-hook-form';
import { useTranslations } from 'use-intl';
import { FormControl, FormField, FormItem } from '@/shared/ui/components/form';
import { FieldRow } from './form-chrome';

/** Whether alerts go out on this integration. Identical for every provider. */
export function EnabledField<T extends FieldValues & { is_enabled: boolean }>({
  disabled,
}: {
  disabled: boolean;
}) {
  const t = useTranslations('alerts');
  const { control } = useFormContext<T>();

  return (
    <FormField
      control={control}
      name={'is_enabled' as Path<T>}
      render={({ field }) => (
        <FormItem>
          <FieldRow
            title={t('integrationDialog.enabled')}
            description={t('integrationDialog.enabledDescription')}
          >
            <FormControl>
              <Switch
                checked={field.value}
                onCheckedChange={field.onChange}
                disabled={disabled}
              />
            </FormControl>
          </FieldRow>
        </FormItem>
      )}
    />
  );
}
