import { Field, FieldError, Skeleton } from '@rustrak/ui';
import { useSelector } from '@tanstack/react-form';
import { lazy, Suspense, useMemo } from 'react';
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
import {
  fieldErrorText,
  revalidateLogic,
  useAppForm,
} from '@/shared/ui/components/app-form';
import { SettingSection } from '@/shared/ui/components/setting-row';
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
  return (
    <Skeleton className="h-[26rem] rounded-md border border-border-field" />
  );
}

/**
 * A stored body was saved as one line; nobody writes JSON that way, so it
 * arrives indented the way they would have written it.
 */
function indentedTemplate(data: CustomWebhookFormData): CustomWebhookFormData {
  return { ...data, template: formatTemplate(data.template) };
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
  const form = useAppForm({
    defaultValues: indentedTemplate(customWebhookDefaults(existingIntegration)),
    validationLogic: revalidateLogic(),
    validators: { onDynamic: schema },
    onSubmit: ({ value }) => submit(value),
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
  const preview = useTemplatePreview(
    useSelector(form.store, (state) => state.values.template),
  );

  const setTemplate = (template: string) =>
    form.setFieldValue('template', template);

  // Replacing a body somebody may have been writing deserves a way back that
  // does not depend on knowing the editor has an undo stack.
  const startFrom = (preset: TemplatePreset) => {
    const previous = form.getFieldValue('template');
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
    <form.AppForm>
      <form.Form>
        <SettingSection title={t('page.general')}>
          <form.AppField name="name">
            {() => (
              <NameField
                placeholder={t('customWebhook.namePlaceholder')}
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
          description={t('customWebhook.description')}
        >
          {(
            [
              {
                name: 'url',
                type: 'url',
                label: t('customWebhook.urlLabel'),
                placeholder: t('customWebhook.urlPlaceholder'),
                description: t('customWebhook.urlDescription'),
              },
              {
                name: 'secret',
                type: 'password',
                label: t('customWebhook.secretLabel'),
                placeholder: t('webhook.secretPlaceholder'),
                description: t('customWebhook.secretDescription'),
              },
            ] as const
          ).map(({ name, ...input }) => (
            <form.AppField key={name} name={name}>
              {() => <TextInputField {...input} disabled={isLoading} />}
            </form.AppField>
          ))}
          <form.AppField name="headers" mode="array">
            {() => <HeadersField disabled={isLoading} />}
          </form.AppField>
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
          <form.Field name="template">
            {(field) => {
              const error = fieldErrorText(field.state.meta.errors);
              return (
                <Field invalid={!!error} className="min-w-0 p-5">
                  <Suspense fallback={<EditorSkeleton />}>
                    <JsonTemplateEditor
                      value={field.state.value}
                      onChange={setTemplate}
                      onBlur={field.handleBlur}
                      onFormat={() =>
                        setTemplate(formatTemplate(field.state.value))
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
                  {error ? <FieldError match>{error}</FieldError> : null}
                </Field>
              );
            }}
          </form.Field>
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
          error={<form.RootError />}
        />
      </form.Form>
    </form.AppForm>
  );
}
