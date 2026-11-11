import { useMemo, useState } from 'react';
import { useTranslations } from 'use-intl';
import { filledCredentials } from '@/features/alert/lib/credentials';
import {
  EMAIL_FIELD_MAP,
  type EmailFormData,
  emailDefaults,
  emailFormSchema,
} from '@/features/alert/model/integration-forms';
import { useIntegrationSubmit } from '@/features/alert/ui/hooks/use-integration-submit';
import { revalidateLogic, useAppForm } from '@/shared/ui/components/app-form';
import { SettingSection } from '@/shared/ui/components/setting-row';
import { ConfigFooter } from '../fields/config-footer';
import { EnabledField } from '../fields/enabled-field';
import { DeleteSection, TestRow } from '../fields/form-chrome';
import { NameField } from '../fields/name-field';
import { TextInputField } from '../fields/text-input-field';
import type { ConfigFormProps } from '../provider-form';

export function EmailForm({
  onOpenChange,
  existingIntegration,
  onTest,
  onDelete,
  isPending: parentPending,
}: ConfigFormProps) {
  const t = useTranslations('alerts');

  const globalT = useTranslations();
  const [testRecipients, setTestRecipients] = useState('');

  // Parsed once so the button's enabled state and its payload can never
  // disagree: input that is only commas or spaces yields an empty list, which
  // the server rejects with "must include at least one recipient".
  const parsedRecipients = useMemo(
    () =>
      testRecipients
        .split(',')
        .map((a) => a.trim())
        .filter(Boolean),
    [testRecipients],
  );

  const schema = useMemo(() => emailFormSchema(t), [t]);
  const form = useAppForm({
    defaultValues: emailDefaults(existingIntegration),
    validationLogic: revalidateLogic(),
    validators: { onDynamic: schema },
    onSubmit: ({ value }) => submit(value),
  });

  const { submit, isPending } = useIntegrationSubmit<EmailFormData>({
    form,
    existingIntegration,
    providerType: 'email',
    credentials: (data) =>
      filledCredentials({
        smtp_host: data.smtp_host,
        smtp_port: data.smtp_port,
        from_address: data.from_address,
        smtp_username: data.smtp_username,
        smtp_password: data.smtp_password,
      }),
    fieldMap: EMAIL_FIELD_MAP,
    labels: {
      name: t('common.fieldName'),
      smtp_host: t('email.fieldHost'),
      smtp_port: t('email.fieldPort'),
      smtp_username: t('email.fieldUsername'),
      smtp_password: t('email.fieldPassword'),
      from_address: t('email.fieldFrom'),
    },
    messages: {
      saveFailed: t('email.saveFailed'),
      created: t('email.created'),
      updated: t('email.updated'),
    },
    t: globalT,
    onSaved: () => onOpenChange(false),
  });

  const isLoading = isPending || parentPending;

  return (
    <form.AppForm>
      <form.Form>
        <SettingSection title={t('page.general')}>
          <form.AppField name="name">
            {() => (
              <NameField
                placeholder={t('email.namePlaceholder')}
                disabled={isLoading}
              />
            )}
          </form.AppField>
          <form.AppField name="is_enabled">
            {() => <EnabledField disabled={isLoading} />}
          </form.AppField>
        </SettingSection>

        <SettingSection
          title={t('page.server')}
          description={t('email.description')}
        >
          <form.AppField name="smtp_host">
            {() => (
              <TextInputField
                label={t('email.hostLabel')}
                placeholder={t('email.hostPlaceholder')}
                disabled={isLoading}
              />
            )}
          </form.AppField>
          <form.AppField name="smtp_port">
            {() => (
              <TextInputField
                label={t('email.portLabel')}
                placeholder={t('email.portPlaceholder')}
                description={t('page.portDescription')}
                numeric
                disabled={isLoading}
              />
            )}
          </form.AppField>
          <form.AppField name="from_address">
            {() => (
              <TextInputField
                label={t('email.fromLabel')}
                placeholder={t('email.fromPlaceholder')}
                description={t('page.fromDescription')}
                type="email"
                disabled={isLoading}
              />
            )}
          </form.AppField>
          {existingIntegration ? (
            <TestRow
              hint={t('email.testHintExisting')}
              placeholder={t('email.testRecipientsPlaceholder')}
              value={testRecipients}
              onValueChange={setTestRecipients}
              disabled={isLoading || parsedRecipients.length === 0}
              onTest={() =>
                onTest(existingIntegration, { recipients: parsedRecipients })
              }
            />
          ) : null}
        </SettingSection>

        <SettingSection
          title={t('page.authentication')}
          description={t('page.authenticationDescription')}
        >
          <form.AppField name="smtp_username">
            {() => (
              <TextInputField
                label={t('email.usernameLabel')}
                placeholder={t('email.usernamePlaceholder')}
                disabled={isLoading}
              />
            )}
          </form.AppField>
          <form.AppField name="smtp_password">
            {() => (
              <TextInputField
                label={t('email.passwordLabel')}
                placeholder={t('email.passwordPlaceholder')}
                type="password"
                disabled={isLoading}
              />
            )}
          </form.AppField>
        </SettingSection>

        {existingIntegration ? (
          <DeleteSection
            integration={existingIntegration}
            disabled={isLoading}
            onDelete={onDelete}
          />
        ) : null}

        <ConfigFooter
          submitLabel={
            existingIntegration ? t('common.saveChanges') : t('email.create')
          }
          isLoading={isLoading}
          onCancel={() => onOpenChange(false)}
          error={<form.RootError />}
        />
      </form.Form>
    </form.AppForm>
  );
}
