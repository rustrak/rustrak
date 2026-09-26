import { zodResolver } from '@hookform/resolvers/zod';
import { Play } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useForm } from 'react-hook-form';
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
import { Button } from '@/shared/ui/components/shadcn/button';
import {
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/shared/ui/components/shadcn/dialog';
import {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  FormRootError,
} from '@/shared/ui/components/shadcn/form';
import { Input } from '@/shared/ui/components/shadcn/input';
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from '@/shared/ui/components/shadcn/tabs';
import { ConfigFooter } from '../fields/config-footer';
import { EnabledField } from '../fields/enabled-field';
import { NameField } from '../fields/name-field';
import { TextInputField } from '../fields/text-input-field';
import type { ConfigFormProps } from '../integration-config-dialog';

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
  const form = useForm<SlackFormData>({
    resolver: zodResolver(schema),
    defaultValues: slackDefaults(existingIntegration),
  });

  const method = form.watch('method') as SlackMethod;

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
  const isBotToken = method === 'bot_token';

  return (
    <>
      <DialogHeader>
        <DialogTitle>
          {t(existingIntegration ? 'slack.titleEdit' : 'slack.titleNew')}
        </DialogTitle>
        <DialogDescription>{t('slack.description')}</DialogDescription>
      </DialogHeader>

      <Form {...form}>
        <form onSubmit={form.handleSubmit(submit)} className="space-y-4">
          <NameField<SlackFormData>
            placeholder={t('slack.namePlaceholder')}
            disabled={isLoading}
          />

          <FormField
            control={form.control}
            name="method"
            render={({ field }) => (
              <FormItem>
                <FormLabel className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
                  {t('slack.methodLabel')}
                </FormLabel>
                <Tabs
                  value={field.value}
                  onValueChange={(v) => field.onChange(v as SlackMethod)}
                >
                  <TabsList className="grid w-full grid-cols-2">
                    <TabsTrigger value="webhook" disabled={isLoading}>
                      {t('slack.incomingWebhook')}
                    </TabsTrigger>
                    <TabsTrigger value="bot_token" disabled={isLoading}>
                      {t('slack.botTokenTab')}
                    </TabsTrigger>
                  </TabsList>

                  <TabsContent value="webhook" className="mt-3">
                    <TextInputField<SlackFormData>
                      name="webhook_url"
                      label={t('slack.webhookUrlLabel')}
                      placeholder={t('slack.webhookUrlPlaceholder')}
                      description={t('slack.webhookUrlDescription')}
                      type="url"
                      disabled={isLoading}
                    />
                  </TabsContent>

                  <TabsContent value="bot_token" className="mt-3">
                    <FormField
                      control={form.control}
                      name="token"
                      render={({ field: tokenField }) => (
                        <FormItem>
                          <FormLabel className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
                            {t('slack.botTokenTab')}
                            {existingIntegration
                              ? ` ${t('slack.tokenLabelSuffix')}`
                              : ''}
                          </FormLabel>
                          <FormControl>
                            <Input
                              type="password"
                              placeholder={t('slack.tokenPlaceholder')}
                              autoComplete="new-password"
                              disabled={isLoading}
                              {...tokenField}
                            />
                          </FormControl>
                          <FormDescription>
                            {t('slack.tokenDescription')}
                          </FormDescription>
                          <FormMessage />
                        </FormItem>
                      )}
                    />
                  </TabsContent>
                </Tabs>
                <FormMessage />
              </FormItem>
            )}
          />

          <EnabledField<SlackFormData> disabled={isLoading} />

          {existingIntegration && (
            <div className="rounded-lg border p-3 space-y-2">
              <p className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
                {t('slack.sendTestTitle')}
              </p>
              {isBotToken ? (
                <>
                  <div className="flex gap-2">
                    <Input
                      placeholder={t('slack.testChannelPlaceholder')}
                      value={testChannel}
                      onChange={(e) => setTestChannel(e.target.value)}
                      className="h-8 text-xs"
                      disabled={isLoading}
                    />
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() => {
                        if (!testChannel.trim()) return;
                        onTest(existingIntegration, {
                          channel: testChannel.trim(),
                        });
                      }}
                      disabled={isLoading || !testChannel.trim()}
                    >
                      <Play className="size-3.5 mr-1" />
                      {t('common.test')}
                    </Button>
                  </div>
                  <p className="text-[11px] text-muted-foreground">
                    {t('slack.testChannelHint')}
                  </p>
                </>
              ) : (
                <div className="flex items-center justify-between">
                  <p className="text-[11px] text-muted-foreground">
                    {t('slack.testWebhookHint')}
                  </p>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => onTest(existingIntegration)}
                    disabled={isLoading}
                  >
                    <Play className="size-3.5 mr-1" />
                    {t('common.test')}
                  </Button>
                </div>
              )}
            </div>
          )}

          <ConfigFooter
            existingIntegration={existingIntegration}
            submitLabel={
              existingIntegration
                ? t('common.saveChanges')
                : t('slack.addIntegration')
            }
            isLoading={isLoading}
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
