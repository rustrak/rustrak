import { zodResolver } from '@hookform/resolvers/zod';
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
import { ConfigFooter } from '../fields/config-footer';
import { EnabledField } from '../fields/enabled-field';
import { NameField } from '../fields/name-field';
import { TextInputField } from '../fields/text-input-field';
import type { ConfigFormProps } from '../integration-config-dialog';

/**
 * The editor is the heaviest thing in the dashboard, and it exists for one
 * field of one provider's dialog. Loaded on demand, that weight is paid by the
 * person who opens this dialog rather than by everyone who visits the
 * integrations page.
 */
const JsonTemplateEditor = lazy(() =>
  import('@/features/alert/ui/components/json-template-editor').then(
    (module) => ({ default: module.JsonTemplateEditor }),
  ),
);

/** Same height and frame as the editor, so the dialog does not jump when the chunk lands. */
function EditorSkeleton() {
  return (
    <div className="h-[26rem] animate-pulse rounded-md border border-input bg-muted/30" />
  );
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
  const { preview, request: requestPreview } = useTemplatePreview();

  // An integration that is being edited already has a body, and the reader
  // should see what it renders to without having to touch it first. One
  // request on mount, not a subscription to the field: both dependencies keep
  // their identity, so this runs once.
  useEffect(() => {
    // A stored body was saved as one line; nobody writes JSON that way, so it
    // arrives indented the way they would have written it.
    const stored = form.getValues('template');
    const indented = formatTemplate(stored);
    if (indented !== stored) form.setValue('template', indented);
    requestPreview(indented);
  }, [form, requestPreview]);

  const setTemplate = (template: string) => {
    form.setValue('template', template, {
      shouldValidate: true,
      shouldDirty: true,
    });
    requestPreview(template);
  };

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
    <>
      <DialogHeader className="shrink-0 border-b px-6 pt-6 pr-14 pb-4">
        <DialogTitle>
          {t(
            existingIntegration
              ? 'customWebhook.titleEdit'
              : 'customWebhook.titleNew',
          )}
        </DialogTitle>
        <DialogDescription>{t('customWebhook.description')}</DialogDescription>
      </DialogHeader>

      <Form {...form}>
        <form
          onSubmit={form.handleSubmit(submit)}
          className="flex min-h-0 flex-1 flex-col"
        >
          {/* The only part that scrolls, so the title and the actions stay
              on screen however long the message body gets. Two columns from
              md up: the small fields on the left, the editor taking the rest,
              because the body is where the reader spends their time. */}
          <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
            <div className="grid gap-x-8 gap-y-5 md:grid-cols-[minmax(0,18rem)_minmax(0,1fr)]">
              <div className="space-y-5">
                <NameField<CustomWebhookFormData>
                  placeholder={t('customWebhook.namePlaceholder')}
                  disabled={isLoading}
                />

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

                <EnabledField<CustomWebhookFormData> disabled={isLoading} />
              </div>

              <FormField
                control={form.control}
                name="template"
                render={({ field }) => (
                  <FormItem className="min-w-0">
                    <FormLabel className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
                      {t('customWebhook.templateLabel')}
                    </FormLabel>
                    <FormControl>
                      <Suspense fallback={<EditorSkeleton />}>
                        <JsonTemplateEditor
                          value={field.value}
                          onChange={setTemplate}
                          onBlur={field.onBlur}
                          onFormat={() =>
                            setTemplate(formatTemplate(field.value))
                          }
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
                    <FormDescription>
                      {t('customWebhook.templateDescription')}
                    </FormDescription>
                    <FormMessage />
                  </FormItem>
                )}
              />
            </div>
          </div>

          {/* Outside the scroll area: a failure that named no field of this
              dialog has to be visible wherever the reader has scrolled to. */}
          <div className="shrink-0 border-t px-6 py-4">
            <FormRootError />
            <ConfigFooter
              existingIntegration={existingIntegration}
              submitLabel={
                existingIntegration
                  ? t('common.saveChanges')
                  : t('customWebhook.create')
              }
              isLoading={isLoading}
              onTest={onTest}
              onDelete={onDelete}
              onCancel={() => onOpenChange(false)}
            />
          </div>
        </form>
      </Form>
    </>
  );
}
