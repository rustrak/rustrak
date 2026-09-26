import { zodResolver } from '@hookform/resolvers/zod';
import { Play } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useForm } from 'react-hook-form';
import { useTranslations } from 'use-intl';
import { filledCredentials } from '@/features/alert/lib/credentials';
import {
  EMAIL_FIELD_MAP,
  type EmailFormData,
  emailDefaults,
  emailFormSchema,
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
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  FormRootError,
} from '@/shared/ui/components/shadcn/form';
import { Input } from '@/shared/ui/components/shadcn/input';
import { ConfigFooter } from '../fields/config-footer';
import { EnabledField } from '../fields/enabled-field';
import { NameField } from '../fields/name-field';
import { TextInputField } from '../fields/text-input-field';
import type { ConfigFormProps } from '../integration-config-dialog';

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
  const form = useForm<EmailFormData>({
    resolver: zodResolver(schema),
    defaultValues: emailDefaults(existingIntegration),
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
    <>
      <DialogHeader>
        <DialogTitle>
          {t(existingIntegration ? 'email.titleEdit' : 'email.titleNew')}
        </DialogTitle>
        <DialogDescription>{t('email.description')}</DialogDescription>
      </DialogHeader>

      <Form {...form}>
        <form onSubmit={form.handleSubmit(submit)} className="space-y-4">
          <NameField<EmailFormData>
            placeholder={t('email.namePlaceholder')}
            disabled={isLoading}
          />

          <div className="grid grid-cols-2 gap-4">
            <FormField
              control={form.control}
              name="smtp_host"
              render={({ field }) => (
                <FormItem>
                  <FormLabel className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
                    {t('email.hostLabel')}
                  </FormLabel>
                  <FormControl>
                    <Input
                      placeholder={t('email.hostPlaceholder')}
                      disabled={isLoading}
                      {...field}
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
              name="smtp_port"
              render={({ field }) => (
                <FormItem>
                  <FormLabel className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
                    {t('email.portLabel')}
                  </FormLabel>
                  <FormControl>
                    <Input
                      type="number"
                      placeholder={t('email.portPlaceholder')}
                      disabled={isLoading}
                      {...field}
                      onChange={(e) =>
                        field.onChange(parseInt(e.target.value, 10) || 587)
                      }
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
          </div>

          <div className="grid grid-cols-2 gap-4">
            <FormField
              control={form.control}
              name="smtp_username"
              render={({ field }) => (
                <FormItem>
                  <FormLabel className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
                    {t('email.usernameLabel')}
                  </FormLabel>
                  <FormControl>
                    <Input
                      placeholder={t('email.usernamePlaceholder')}
                      autoComplete="off"
                      disabled={isLoading}
                      {...field}
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
              name="smtp_password"
              render={({ field }) => (
                <FormItem>
                  <FormLabel className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
                    {t('email.passwordLabel')}
                  </FormLabel>
                  <FormControl>
                    <Input
                      type="password"
                      placeholder={t('email.passwordPlaceholder')}
                      autoComplete="new-password"
                      disabled={isLoading}
                      {...field}
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
          </div>

          <TextInputField<EmailFormData>
            name="from_address"
            label={t('email.fromLabel')}
            placeholder={t('email.fromPlaceholder')}
            type="email"
            disabled={isLoading}
          />

          <EnabledField<EmailFormData> disabled={isLoading} />

          {/* Shown while creating too, disabled — the test endpoint needs a
                persisted integration id, so it only becomes usable on save. */}
          <div className="rounded-lg border p-3 space-y-2">
            <p className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
              {t('email.sendTestTitle')}
            </p>
            <div className="flex gap-2">
              <Input
                aria-label={t('email.testRecipientsAria')}
                placeholder={t('email.testRecipientsPlaceholder')}
                value={testRecipients}
                onChange={(e) => setTestRecipients(e.target.value)}
                className="h-8 text-xs"
                disabled={isLoading || !existingIntegration}
              />
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => {
                  if (!existingIntegration || parsedRecipients.length === 0) {
                    return;
                  }
                  onTest(existingIntegration, {
                    recipients: parsedRecipients,
                  });
                }}
                disabled={
                  isLoading ||
                  !existingIntegration ||
                  parsedRecipients.length === 0
                }
              >
                <Play className="size-3.5 mr-1" />
                {t('common.test')}
              </Button>
            </div>
            <p className="text-[11px] text-muted-foreground">
              {existingIntegration
                ? t('email.testHintExisting')
                : t('email.testHintNew')}
            </p>
          </div>

          <ConfigFooter
            existingIntegration={existingIntegration}
            submitLabel={
              existingIntegration ? t('common.saveChanges') : t('email.create')
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
