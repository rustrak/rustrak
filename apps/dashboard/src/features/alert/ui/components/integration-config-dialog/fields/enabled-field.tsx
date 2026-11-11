import { Switch } from '@rustrak/ui';
import { useId } from 'react';
import { useTranslations } from 'use-intl';
import { useFieldContext } from '@/shared/ui/components/app-form';
import { SettingRow } from '@/shared/ui/components/setting-row';

/**
 * Whether alerts go out on this integration. Identical for every provider.
 * Rendered inside `form.AppField name="is_enabled"`.
 */
export function EnabledField({ disabled }: { disabled: boolean }) {
  const t = useTranslations('alerts');
  const field = useFieldContext<boolean>();
  const id = useId();

  return (
    <SettingRow
      htmlFor={id}
      title={t('integrationDialog.enabled')}
      description={t('integrationDialog.enabledDescription')}
    >
      <Switch
        id={id}
        checked={field.state.value}
        onCheckedChange={field.handleChange}
        disabled={disabled}
      />
    </SettingRow>
  );
}
