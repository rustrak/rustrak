import { zodResolver } from '@hookform/resolvers/zod';
import { useMemo } from 'react';
import { useForm } from 'react-hook-form';
import { useTranslations } from 'use-intl';
import { filledCredentials } from '@/features/alert/lib/credentials';
import {
  headersFromRows,
  WEBHOOK_FIELD_MAP,
  type WebhookFormData,
  webhookDefaults,
  webhookFormSchema,
} from '@/features/alert/model/integration-forms';
import { useIntegrationSubmit } from '@/features/alert/ui/hooks/use-integration-submit';
import { Form } from '@/shared/ui/components/form';
import { SettingSection } from '@/shared/ui/components/setting-row';
import { ConfigFooter } from '../fields/config-footer';
import { EnabledField } from '../fields/enabled-field';
import { DeleteSection, TestRow } from '../fields/form-chrome';
import { HeadersField } from '../fields/headers-field';
import { NameField } from '../fields/name-field';
import { TextInputField } from '../fields/text-input-field';
import type { ConfigFormProps } from '../provider-form';

export function WebhookForm({
  onOpenChange,
  existingIntegration,
  onTest,
  onDelete,
  isPending: parentPending,
}: ConfigFormProps) {
  const t = useTranslations('alerts');

  const globalT = useTranslations();

  // Seeded at mount, never re-seeded: the page keys the form by the
  // integration it edits, so another one is a fresh mount.
  const schema = useMemo(() => webhookFormSchema(t), [t]);
  const form = useForm<WebhookFormData>({
    resolver: zodResolver(schema),
    defaultValues: webhookDefaults(existingIntegration),
  });

  const { submit, isPending } = useIntegrationSubmit<WebhookFormData>({
    form,
    existingIntegration,
    providerType: 'webhook',
    credentials: (data) =>
      filledCredentials({
        url: data.url,
        secret: data.secret,
        headers: headersFromRows(data.headers),
      }),
    fieldMap: WEBHOOK_FIELD_MAP,
    labels: {
      name: t('common.fieldName'),
      url: t('webhook.fieldUrl'),
      secret: t('webhook.fieldSecret'),
    },
    messages: {
      saveFailed: t('webhook.saveFailed'),
      created: t('webhook.created'),
      updated: t('webhook.updated'),
    },
    t: globalT,
    onSaved: () => onOpenChange(false),
  });

  const isLoading = isPending || parentPending;

  return (
    <Form {...form}>
      <form onSubmit={form.handleSubmit(submit)}>
        <SettingSection title={t('page.general')}>
          <NameField<WebhookFormData>
            placeholder={t('webhook.namePlaceholder')}
            disabled={isLoading}
          />
          <EnabledField<WebhookFormData> disabled={isLoading} />
        </SettingSection>

        <SettingSection
          title={t('page.connection')}
          description={t('webhook.description')}
        >
          <TextInputField<WebhookFormData>
            name="url"
            label={t('webhook.urlLabel')}
            placeholder={t('webhook.urlPlaceholder')}
            description={t('webhook.urlDescription')}
            type="url"
            disabled={isLoading}
          />
          <TextInputField<WebhookFormData>
            name="secret"
            label={t('webhook.secretLabel')}
            placeholder={t('webhook.secretPlaceholder')}
            description={t('webhook.secretDescription')}
            type="password"
            disabled={isLoading}
          />
          <HeadersField<WebhookFormData> disabled={isLoading} />
          {existingIntegration ? (
            <TestRow
              hint={t('page.testDescription')}
              disabled={isLoading}
              onTest={() => onTest(existingIntegration)}
            />
          ) : null}
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
            existingIntegration ? t('common.saveChanges') : t('webhook.create')
          }
          isLoading={isLoading}
          onCancel={() => onOpenChange(false)}
        />
      </form>
    </Form>
  );
}
