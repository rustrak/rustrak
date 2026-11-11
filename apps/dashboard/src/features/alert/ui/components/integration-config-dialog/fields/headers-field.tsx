import {
  Button,
  DeleteIcon,
  Field,
  FieldError,
  Input,
  NewIcon,
} from '@rustrak/ui';
import { useSelector } from '@tanstack/react-form';
import { useState } from 'react';
import { useTranslations } from 'use-intl';
import type { HeaderRow } from '@/features/alert/model/integration-forms';
import {
  fieldErrorText,
  useFieldContext,
} from '@/shared/ui/components/app-form';
import { SettingRow } from '@/shared/ui/components/setting-row';

/**
 * The extra HTTP headers a webhook request carries, as name/value rows: an
 * `Authorization` for an API that wants one (#419). Rendered inside
 * `form.AppField name="headers" mode="array"`.
 */
export function HeadersField({ disabled }: { disabled: boolean }) {
  const t = useTranslations('alerts');
  const field = useFieldContext<HeaderRow[]>();
  const rows = field.state.value;
  // The schema reports a bad row at `headers[i].name`; no field is mounted
  // there, so the message is read off the form.
  const nameErrors = useSelector(field.form.store, (state) =>
    rows
      .map((_, index) =>
        fieldErrorText(state.fieldMeta[`headers[${index}].name`]?.errors ?? []),
      )
      .join('\u0000'),
  ).split('\u0000');

  // Rows have no identity of their own; these keys follow a row as others
  // are removed, so focus stays with it. Rows only change through here.
  const [keys, setKeys] = useState(() => rows.map((_, index) => index));
  const [added, setAdded] = useState<number>();

  const setRow = (index: number, row: HeaderRow) =>
    field.replaceValue(index, row);
  const addRow = () => {
    const key = Math.max(-1, ...keys) + 1;
    setKeys((prev) => [...prev, key]);
    setAdded(key);
    field.pushValue({ name: '', value: '' });
  };
  const removeRow = (index: number) => {
    setKeys((prev) => prev.filter((_, at) => at !== index));
    field.removeValue(index);
  };

  return (
    <SettingRow
      title={t('headers.label')}
      description={t('headers.description')}
      wide
    >
      <div className="flex flex-col gap-2">
        {rows.map((row, index) => {
          const nameError = nameErrors[index] || undefined;
          return (
            <div
              key={keys[index]}
              className="grid grid-cols-[1fr_1fr_auto] gap-2"
            >
              <Field invalid={!!nameError}>
                <Input
                  aria-label={t('headers.nameLabel')}
                  autoFocus={keys[index] === added}
                  placeholder={t('headers.namePlaceholder')}
                  disabled={disabled}
                  autoComplete="off"
                  className="font-mono"
                  invalid={!!nameError}
                  value={row.name}
                  onChange={(e) =>
                    setRow(index, { ...row, name: e.target.value })
                  }
                  onBlur={field.handleBlur}
                />
                {nameError ? <FieldError match>{nameError}</FieldError> : null}
              </Field>
              <Field>
                {/* Usually a token, so hidden like the signing secret. */}
                <Input
                  type="password"
                  aria-label={t('headers.valueLabel')}
                  placeholder={t('headers.valuePlaceholder')}
                  disabled={disabled}
                  autoComplete="new-password"
                  value={row.value}
                  onChange={(e) =>
                    setRow(index, { ...row, value: e.target.value })
                  }
                  onBlur={field.handleBlur}
                />
              </Field>
              <Button
                type="button"
                variant="ghost"
                icon={DeleteIcon}
                onClick={() => removeRow(index)}
                disabled={disabled}
                aria-label={t('headers.remove')}
              />
            </div>
          );
        })}

        <Button
          type="button"
          variant="secondary"
          size="sm"
          icon={NewIcon}
          className="self-start"
          onClick={addRow}
          disabled={disabled}
        >
          {t('headers.add')}
        </Button>
      </div>
    </SettingRow>
  );
}
