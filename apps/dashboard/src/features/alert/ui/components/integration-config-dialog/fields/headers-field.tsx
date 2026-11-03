import { Button, DeleteIcon, Input, NewIcon } from '@rustrak/ui';
import {
  type ArrayPath,
  type FieldValues,
  type Path,
  useFieldArray,
  useFormContext,
} from 'react-hook-form';
import { useTranslations } from 'use-intl';
import {
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from '@/shared/ui/components/form';
import { SettingRow } from '@/shared/ui/components/setting-row';

/**
 * The extra HTTP headers a webhook request carries, as name/value rows: an
 * `Authorization` for an API that wants one (#419).
 *
 * Generic over the form shape like `TextInputField`; the form's `headers`
 * field must be an array of `{ name, value }`.
 */
export function HeadersField<T extends FieldValues>({
  disabled,
}: {
  disabled: boolean;
}) {
  const t = useTranslations('alerts');
  const { control } = useFormContext<T>();
  const { fields, append, remove } = useFieldArray({
    control,
    name: 'headers' as ArrayPath<T>,
  });

  return (
    <SettingRow
      title={t('headers.label')}
      description={t('headers.description')}
      wide
    >
      <div className="flex flex-col gap-2">
        {fields.map((row, index) => (
          <div key={row.id} className="grid grid-cols-[1fr_1fr_auto] gap-2">
            <FormField
              control={control}
              name={`headers.${index}.name` as Path<T>}
              render={({ field }) => (
                <FormItem>
                  <FormLabel className="sr-only">
                    {t('headers.nameLabel')}
                  </FormLabel>
                  <FormControl>
                    <Input
                      placeholder={t('headers.namePlaceholder')}
                      disabled={disabled}
                      autoComplete="off"
                      className="font-mono"
                      {...field}
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <FormField
              control={control}
              name={`headers.${index}.value` as Path<T>}
              render={({ field }) => (
                <FormItem>
                  <FormLabel className="sr-only">
                    {t('headers.valueLabel')}
                  </FormLabel>
                  <FormControl>
                    {/* Usually a token, so hidden like the signing secret. */}
                    <Input
                      type="password"
                      placeholder={t('headers.valuePlaceholder')}
                      disabled={disabled}
                      autoComplete="new-password"
                      {...field}
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <Button
              type="button"
              variant="ghost"
              icon={DeleteIcon}
              onClick={() => remove(index)}
              disabled={disabled}
              aria-label={t('headers.remove')}
            />
          </div>
        ))}

        <Button
          type="button"
          variant="secondary"
          size="sm"
          icon={NewIcon}
          className="self-start"
          onClick={() =>
            append({ name: '', value: '' } as Parameters<typeof append>[0])
          }
          disabled={disabled}
        >
          {t('headers.add')}
        </Button>
      </div>
    </SettingRow>
  );
}
