import { SegmentedControl, SegmentedItem } from '@rustrak/ui';
import { useSelector } from '@tanstack/react-form';
import { useMemo, useState } from 'react';
import { useTranslations } from 'use-intl';
import { filledCredentials } from '@/features/alert/lib/credentials';
import {
  SLACK_FIELD_MAP,
  type SlackFormData,
  type SlackMethod,
  slackDefaults,
  slackFormSchema,
} from '@/features/alert/model/integration-forms';
import { useIntegrationSubmit } from '@/features/alert/ui/hooks/use-integration-submit';
import { revalidateLogic, useAppForm } from '@/shared/ui/components/app-form';
import { SettingRow, SettingSection } from '@/shared/ui/components/setting-row';
import { ConfigFooter } from '../fields/config-footer';
import { EnabledField } from '../fields/enabled-field';
import { DeleteSection, TestRow } from '../fields/form-chrome';
import { NameField } from '../fields/name-field';
import { TextInputField } from '../fields/text-input-field';
import type { ConfigFormProps } from '../provider-form';

export function SlackForm({
  onOpenChange,
  existingIntegration,
  onTest,
  onDelete,
  isPending: parentPending,
}: ConfigFormProps) {
  const t = useTranslations('alerts');

  const globalT = useTranslations();
  const [testChannel, setTestChannel] = useState('');

  const schema = useMemo(() => slackFormSchema(t), [t]);
  const form = useAppForm({
    defaultValues: slackDefaults(existingIntegration),
    validationLogic: revalidateLogic(),
    validators: { onDynamic: schema },
    onSubmit: ({ value }) => submit(value),
  });

  const method = useSelector(form.store, (state) => state.values.method);

  const { submit, isPending } = useIntegrationSubmit<SlackFormData>({
    form,
    existingIntegration,
    providerType: 'slack',
    // The two methods are alternatives, not a union of fields: an incoming
    // webhook has no token and a bot has no webhook URL, and sending the
    // unused one would leave the integration describing itself as both. A
    // blank token on edit falls out of `filledCredentials`, which is what
    // "leave the stored token alone" looks like on the wire.
    credentials: (data) =>
      data.method === 'webhook'
        ? { method: 'webhook', webhook_url: data.webhook_url }
        : filledCredentials({ method: 'bot_token', token: data.token }),
    fieldMap: SLACK_FIELD_MAP,
    labels: {
      name: t('common.fieldName'),
      webhook_url: t('slack.fieldWebhookUrl'),
      token: t('slack.fieldToken'),
    },
    messages: {
      saveFailed: t('slack.saveFailed'),
      created: t('slack.created'),
      updated: t('slack.updated'),
    },
    t: globalT,
    onSaved: () => onOpenChange(false),
  });

  const isLoading = isPending || parentPending;
  const channel = testChannel.trim();

  return (
    <form.AppForm>
      <form.Form>
        <SettingSection title={t('page.general')}>
          <form.AppField name="name">
            {() => (
              <NameField
                placeholder={t('slack.namePlaceholder')}
                disabled={isLoading}
              />
            )}
          </form.AppField>
          <form.AppField name="is_enabled">
            {() => <EnabledField disabled={isLoading} />}
          </form.AppField>
        </SettingSection>

        <SettingSection
          title={t('page.connection')}
          description={t('slack.description')}
        >
          <SettingRow
            title={t('slack.methodLabel')}
            description={t('page.methodDescription')}
          >
            <SegmentedControl
              aria-label={t('slack.methodLabel')}
              value={method}
              onValueChange={(v) =>
                form.setFieldValue('method', v as SlackMethod)
              }
              disabled={isLoading}
            >
              <SegmentedItem value="webhook">
                {t('slack.incomingWebhook')}
              </SegmentedItem>
              <SegmentedItem value="bot_token">
                {t('slack.botTokenTab')}
              </SegmentedItem>
            </SegmentedControl>
          </SettingRow>

          {method === 'webhook' ? (
            <form.AppField key="webhook_url" name="webhook_url">
              {() => (
                <TextInputField
                  label={t('slack.webhookUrlLabel')}
                  placeholder={t('slack.webhookUrlPlaceholder')}
                  description={t('slack.webhookUrlDescription')}
                  type="url"
                  disabled={isLoading}
                />
              )}
            </form.AppField>
          ) : (
            <form.AppField key="token" name="token">
              {() => (
                <TextInputField
                  label={
                    existingIntegration
                      ? `${t('slack.botTokenTab')} ${t('slack.tokenLabelSuffix')}`
                      : t('slack.botTokenTab')
                  }
                  placeholder={t('slack.tokenPlaceholder')}
                  description={t('slack.tokenDescription')}
                  type="password"
                  disabled={isLoading}
                />
              )}
            </form.AppField>
          )}

          {existingIntegration ? (
            method === 'bot_token' ? (
              <TestRow
                hint={t('slack.testChannelHint')}
                placeholder={t('slack.testChannelPlaceholder')}
                value={testChannel}
                onValueChange={setTestChannel}
                disabled={isLoading || !channel}
                onTest={() => onTest(existingIntegration, { channel })}
              />
            ) : (
              <TestRow
                hint={t('slack.testWebhookHint')}
                disabled={isLoading}
                onTest={() => onTest(existingIntegration)}
              />
            )
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
            existingIntegration
              ? t('common.saveChanges')
              : t('slack.addIntegration')
          }
          isLoading={isLoading}
          onCancel={() => onOpenChange(false)}
          error={<form.RootError />}
        />
      </form.Form>
    </form.AppForm>
  );
}
