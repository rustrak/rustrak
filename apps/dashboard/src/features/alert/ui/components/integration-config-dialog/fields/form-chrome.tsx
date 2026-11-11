import type { AlertIntegration } from '@rustrak/client';
import {
  Button,
  DeleteIcon,
  Field,
  FieldError,
  Input,
  TestIcon,
} from '@rustrak/ui';
import { type ReactNode, useId } from 'react';
import { useTranslations } from 'use-intl';
import {
  fieldErrorText,
  useFieldContext,
} from '@/shared/ui/components/app-form';
import { SettingRow, SettingSection } from '@/shared/ui/components/setting-row';

/*
 * The parts a provider's form is built from, now that it is a settings page
 * rather than a dialog: rows inside sections, like every other settings page,
 * instead of a column of stacked labels.
 */

/**
 * A `SettingRow` for the form field it sits in: its title is the field's
 * label, and the field's error goes under the control `children` renders
 * with the id it is given.
 */
export function FieldRow({
  children,
  ...props
}: {
  title: string;
  description?: string;
  wide?: boolean;
  children: (id: string) => ReactNode;
}) {
  const id = useId();
  const field = useFieldContext<unknown>();
  const error = fieldErrorText(field.state.meta.errors);
  return (
    <SettingRow htmlFor={id} {...props}>
      <Field invalid={!!error}>
        {children(id)}
        {error ? <FieldError match>{error}</FieldError> : null}
      </Field>
    </SettingRow>
  );
}

/**
 * Sending a sample alert through the saved integration. An input when the
 * provider needs to know where to send it -- a Slack channel, a few addresses
 * -- and just the button when the integration already knows.
 *
 * It tests what is stored, not the draft, which is why it only exists once
 * the integration does.
 */
export function TestRow({
  hint,
  value,
  onValueChange,
  placeholder,
  disabled,
  onTest,
}: {
  hint: string;
  /** With these three, the row asks where to send the test. */
  value?: string;
  onValueChange?: (value: string) => void;
  placeholder?: string;
  disabled: boolean;
  onTest: () => void;
}) {
  const t = useTranslations('alerts');
  const asks = onValueChange !== undefined;
  return (
    <SettingRow title={t('page.testTitle')} description={hint}>
      <div className="flex gap-2">
        {asks ? (
          <Input
            aria-label={t('page.testTitle')}
            placeholder={placeholder}
            value={value}
            onChange={(e) => onValueChange(e.target.value)}
            onKeyDown={(e) => {
              // The Enter that confirms an IME candidate is not a send.
              if (e.key === 'Enter' && !e.nativeEvent.isComposing) {
                // Enter here sends the test, not the whole form.
                e.preventDefault();
                if (!disabled) onTest();
              }
            }}
            boxClassName="min-w-0 flex-1"
          />
        ) : null}
        <Button
          type="button"
          variant="secondary"
          icon={TestIcon}
          onClick={onTest}
          disabled={disabled}
        >
          {t('common.test')}
        </Button>
      </div>
    </SettingRow>
  );
}

/**
 * Deleting the integration, apart from the form: everything above it can be
 * typed back, this cannot. The confirmation is the page's.
 */
export function DeleteSection({
  integration,
  disabled,
  onDelete,
}: {
  integration: AlertIntegration;
  disabled: boolean;
  /** Absent: the reader may not delete it, so nothing is offered. */
  onDelete?: (integration: AlertIntegration) => void;
}) {
  const t = useTranslations('alerts');
  if (!onDelete) return null;
  return (
    <SettingSection title={t('page.dangerTitle')} destructive>
      <SettingRow
        title={t('page.deleteTitle')}
        description={t('page.deleteDescription')}
      >
        <Button
          type="button"
          variant="danger"
          icon={DeleteIcon}
          onClick={() => onDelete(integration)}
          disabled={disabled}
        >
          {t('common.delete')}
        </Button>
      </SettingRow>
    </SettingSection>
  );
}
