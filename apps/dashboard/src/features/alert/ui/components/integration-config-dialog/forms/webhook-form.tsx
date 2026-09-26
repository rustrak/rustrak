import { zodResolver } from '@hookform/resolvers/zod';
import { useMemo } from 'react';
import { useForm } from 'react-hook-form';
import { useTranslations } from 'use-intl';
import { filledCredentials } from '@/features/alert/lib/credentials';
import {
  WEBHOOK_FIELD_MAP,
  type WebhookFormData,
  webhookDefaults,
  webhookFormSchema,
} from '@/features/alert/model/integration-forms';
import { useIntegrationSubmit } from '@/features/alert/ui/hooks/use-integration-submit';
import {
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/shared/ui/components/shadcn/dialog';
import { Form, FormRootError } from '@/shared/ui/components/shadcn/form';
import { ConfigFooter } from '../fields/config-footer';
import { EnabledField } from '../fields/enabled-field';
import { NameField } from '../fields/name-field';
import { TextInputField } from '../fields/text-input-field';
import type { ConfigFormProps } from '../integration-config-dialog';

export function WebhookForm({
  onOpenChange,
  existingIntegration,
  onTest,
  onDelete,
  isPending: parentPending,
}: ConfigFormProps) {
  const t = useTranslations('alerts');

  const globalT = useTranslations();

  // Seeded at mount, never re-seeded. The body only exists while the dialog
  // is open (Base UI unmounts the portal after the close animation), so
  // "opened again" and "opened on a different integration" are both a fresh
  // mount, and the `key` on the shell makes the second case structural rather
  // than a fact about how the parent sequences its state updates.
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
      filledCredentials({ url: data.url, secret: data.secret }),
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
    <>
      <DialogHeader>
        <DialogTitle>
          {t(existingIntegration ? 'webhook.titleEdit' : 'webhook.titleNew')}
        </DialogTitle>
        <DialogDescription>{t('webhook.description')}</DialogDescription>
      </DialogHeader>

      <Form {...form}>
        <form onSubmit={form.handleSubmit(submit)} className="space-y-4">
          <NameField<WebhookFormData>
            placeholder={t('webhook.namePlaceholder')}
            disabled={isLoading}
          />

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

          <EnabledField<WebhookFormData> disabled={isLoading} />

          <ConfigFooter
            existingIntegration={existingIntegration}
            submitLabel={
              existingIntegration
                ? t('common.saveChanges')
                : t('webhook.create')
            }
            isLoading={isLoading}
            onTest={onTest}
            onDelete={onDelete}
            onCancel={() => onOpenChange(false)}
          />
          {/* Where a failure that named no field of this dialog lands. */}
          <FormRootError />
        </form>
      </Form>
    </>
  );
}
