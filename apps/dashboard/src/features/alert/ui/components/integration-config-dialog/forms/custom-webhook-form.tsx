import { zodResolver } from '@hookform/resolvers/zod';
import { Skeleton } from '@rustrak/ui';
import { lazy, Suspense, useEffect, useMemo } from 'react';
import { useForm } from 'react-hook-form';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { filledCredentials } from '@/features/alert/lib/credentials';
import { formatTemplate } from '@/features/alert/lib/format-template';
import {
  CUSTOM_WEBHOOK_FIELD_MAP,
  type CustomWebhookFormData,
  customWebhookDefaults,
  customWebhookFormSchema,
  headersFromRows,
} from '@/features/alert/model/integration-forms';
import {
  TEMPLATE_DOCS_URL,
  TEMPLATE_PRESETS,
  TEMPLATE_VARIABLES,
  type TemplatePreset,
  templatePlaceholder,
} from '@/features/alert/model/message-template';
import { useIntegrationSubmit } from '@/features/alert/ui/hooks/use-integration-submit';
import { useTemplatePreview } from '@/features/alert/ui/hooks/use-template-preview';
import { SettingSection } from '@/shared/ui/components/setting-row';
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormMessage,
} from '@/shared/ui/components/shadcn/form';
import { ConfigFooter } from '../fields/config-footer';
import { EnabledField } from '../fields/enabled-field';
import { DeleteSection, TestRow } from '../fields/form-chrome';
import { HeadersField } from '../fields/headers-field';
import { NameField } from '../fields/name-field';
import { TextInputField } from '../fields/text-input-field';
import type { ConfigFormProps } from '../provider-form';

/**
 * The editor is the heaviest thing in the dashboard, and it exists for one
 * field of one provider's form. Loaded on demand, that weight is paid by the
 * person who opens this form rather than by everyone who visits the
 * integrations page.
 */
const JsonTemplateEditor = lazy(() =>
  import('@/features/alert/ui/components/json-template-editor').then(
    (module) => ({ default: module.JsonTemplateEditor }),
  ),
);

/** Same height and frame as the editor, so the page does not jump when the chunk lands. */
function EditorSkeleton() {
  return <Skeleton className="h-[26rem] rounded-md border border-input" />;
}

export function CustomWebhookForm({
  onOpenChange,
  existingIntegration,
  onTest,
  onDelete,
  isPending: parentPending,
}: ConfigFormProps) {
  const t = useTranslations('alerts');

  const globalT = useTranslations();

  const schema = useMemo(() => customWebhookFormSchema(t), [t]);
  const form = useForm<CustomWebhookFormData>({
    resolver: zodResolver(schema),
    defaultValues: customWebhookDefaults(existingIntegration),
  });

  const { submit, isPending } = useIntegrationSubmit<CustomWebhookFormData>({
    form,
    existingIntegration,
    providerType: 'custom_webhook',
    credentials: (data) =>
      filledCredentials({
        url: data.url,
        secret: data.secret,
        template: data.template,
        headers: headersFromRows(data.headers),
      }),
    fieldMap: CUSTOM_WEBHOOK_FIELD_MAP,
    labels: {
      name: t('common.fieldName'),
      url: t('webhook.fieldUrl'),
      secret: t('webhook.fieldSecret'),
      template: t('customWebhook.fieldTemplate'),
    },
    messages: {
      saveFailed: t('customWebhook.saveFailed'),
      created: t('customWebhook.created'),
      updated: t('customWebhook.updated'),
    },
    t: globalT,
    onSaved: () => onOpenChange(false),
  });

  const isLoading = isPending || parentPending;

  // The server renders the body against a sample alert a moment after each
  // change; the editor shows the answer underneath and underlines a refusal.
  // An integration being edited already has a body, so the reader sees what
  // it renders to without having to touch it first.
  const preview = useTemplatePreview(form.watch('template'));

  // A stored body was saved as one line; nobody writes JSON that way, so it
  // arrives indented the way they would have written it. `form` keeps its
  // identity, so this runs once.
  useEffect(() => {
    const stored = form.getValues('template');
    const indented = formatTemplate(stored);
    if (indented !== stored) form.setValue('template', indented);
  }, [form]);

  const setTemplate = (template: string) =>
    form.setValue('template', template, {
      shouldValidate: true,
      shouldDirty: true,
    });

  // Replacing a body somebody may have been writing deserves a way back that
  // does not depend on knowing the editor has an undo stack.
  const startFrom = (preset: TemplatePreset) => {
    const previous = form.getValues('template');
    setTemplate(preset.body);
    if (!previous.trim() || previous === preset.body) return;
    toast(t('customWebhook.presetApplied', { name: preset.name }), {
      action: {
        label: t('customWebhook.undo'),
        onClick: () => setTemplate(previous),
      },
    });
  };

  return (
    <Form {...form}>
      <form onSubmit={form.handleSubmit(submit)}>
        <SettingSection title={t('page.general')}>
          <NameField<CustomWebhookFormData>
            placeholder={t('customWebhook.namePlaceholder')}
            disabled={isLoading}
          />
          <EnabledField<CustomWebhookFormData> disabled={isLoading} />
        </SettingSection>

        <SettingSection
          title={t('page.connection')}
          description={t('customWebhook.description')}
        >
          <TextInputField<CustomWebhookFormData>
            name="url"
            label={t('customWebhook.urlLabel')}
            placeholder={t('customWebhook.urlPlaceholder')}
            description={t('customWebhook.urlDescription')}
            type="url"
            disabled={isLoading}
          />
          <TextInputField<CustomWebhookFormData>
            name="secret"
            label={t('customWebhook.secretLabel')}
            placeholder={t('webhook.secretPlaceholder')}
            description={t('customWebhook.secretDescription')}
            type="password"
            disabled={isLoading}
          />
          <HeadersField<CustomWebhookFormData> disabled={isLoading} />
          {existingIntegration ? (
            <TestRow
              hint={t('page.testDescription')}
              disabled={isLoading}
              onTest={() => onTest(existingIntegration)}
            />
          ) : null}
        </SettingSection>

        {/* The body is where the reader spends their time, so it gets the
            page's full width rather than a column beside the small fields. */}
        <SettingSection
          title={t('customWebhook.templateLabel')}
          description={t('customWebhook.templateDescription')}
        >
          <FormField
            control={form.control}
            name="template"
            render={({ field }) => (
              <FormItem className="min-w-0 p-5">
                <FormControl>
                  <Suspense fallback={<EditorSkeleton />}>
                    <JsonTemplateEditor
                      value={field.value}
                      onChange={setTemplate}
                      onBlur={field.onBlur}
                      onFormat={() => setTemplate(formatTemplate(field.value))}
                      onPreset={startFrom}
                      preview={preview}
                      disabled={isLoading}
                      placeholder={templatePlaceholder}
                      variables={TEMPLATE_VARIABLES}
                      presets={TEMPLATE_PRESETS}
                      ariaLabel={t('customWebhook.templateLabel')}
                      helpHref={TEMPLATE_DOCS_URL}
                    />
                  </Suspense>
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />
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
              : t('customWebhook.create')
          }
          isLoading={isLoading}
          onCancel={() => onOpenChange(false)}
        />
      </form>
    </Form>
  );
}
