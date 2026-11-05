import { zodResolver } from '@hookform/resolvers/zod';
import type { ProjectResponse } from '@rustrak/client';
import { useMutation } from '@tanstack/react-query';
import { useMemo } from 'react';
import { useForm } from 'react-hook-form';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { z } from 'zod';
import { projectMutations } from '@/features/project/api/mutations';
import {
  projectNameField,
  projectSlugField,
} from '@/features/project/model/fields';
import { ApiError } from '@/shared/api/query';
import type { Translate } from '@/shared/lib/error-copy';
import { describeError } from '@/shared/lib/error-copy';
import {
  applyServerFieldErrors,
  SERVER_ERROR_PATH,
} from '@/shared/lib/form-errors';
import { Form, FormRootError } from '@/shared/ui/components/form';
import { PlatformPicker } from '@/shared/ui/components/platform-picker';
import { SettingRow, SettingSection } from '@/shared/ui/components/setting-row';
import { useRouter } from '@/shared/ui/hooks/use-router';
import { DangerZone } from './danger-zone';
import { SavableRow } from './savable-row';

/**
 * The same rules the create form uses, imported rather than restated.
 *
 * This form had none at all before: it was raw `useState`, so a one-character
 * name reached the server and came back as a toast, and a server-named field
 * had no input to attach itself to.
 */
function buildGeneralSettingsSchema(t: Translate) {
  return z.object({
    name: projectNameField(t),
    slug: projectSlugField(t),
  });
}

export type GeneralSettingsFormData = z.infer<
  ReturnType<typeof buildGeneralSettingsSchema>
>;

interface GeneralSettingsFormProps {
  project: ProjectResponse;
  /** `project:update`. */
  canUpdate: boolean;
  /** `project:delete`: without it there is no danger zone. */
  canDelete: boolean;
}

export function GeneralSettingsForm({
  project,
  canUpdate,
  canDelete,
}: GeneralSettingsFormProps) {
  const t = useTranslations('projects');
  const formT = useTranslations();
  const router = useRouter();
  const update = useMutation(projectMutations.update(project.id));
  const remove = useMutation(projectMutations.delete());
  const isPending = update.isPending;

  const generalSettingsSchema = useMemo(
    () => buildGeneralSettingsSchema(t),
    [t],
  );

  const form = useForm<GeneralSettingsFormData>({
    resolver: zodResolver(generalSettingsSchema),
    defaultValues: { name: project.name, slug: project.slug },
  });

  const name = form.watch('name');
  const slug = form.watch('slug');
  const hasNameChanges = name !== project.name;
  const hasSlugChanges = slug !== project.slug;

  const handleSaveName = () => {
    // Each row saves one field, so validate that field alone. Submitting the
    // whole form would mark the slug red for a name the user never touched.
    void form.trigger('name').then((valid) => {
      if (!valid || !hasNameChanges) return;

      const trimmed = form.getValues('name').trim();
      update.mutate(
        { name: trimmed },
        {
          onSuccess: () => {
            // Keep the field on the value that was actually persisted,
            // otherwise stray whitespace leaves the Save button looking
            // permanently dirty.
            form.setValue('name', trimmed);
            clearFailure();
            toast.success(t('toasts.nameUpdated'));
          },
          onError: (error) =>
            reportFailure(error.error, t('toasts.updateNameFailed')),
        },
      );
    });
  };

  const handleSaveSlug = () => {
    void form.trigger('slug').then((valid) => {
      if (!valid || !hasSlugChanges) return;

      update.mutate(
        { slug: form.getValues('slug').trim() },
        {
          onSuccess: (updated) => {
            // The server slugifies the input, so echo back what it actually
            // stored rather than the raw text: typing "My API" must leave the
            // field showing "my-api", not a value that was never persisted.
            form.setValue('slug', updated.slug);
            clearFailure();
            toast.success(t('toasts.slugUpdated'));
          },
          onError: (error) =>
            reportFailure(error.error, t('toasts.updateSlugFailed')),
        },
      );
    });
  };

  const handlePlatformChange = (platform: string) => {
    if (!platform || platform === project.platform) return;

    update.mutate(
      { platform },
      {
        onSuccess: () => {
          clearFailure();
          toast.success(t('toasts.platformUpdated'));
        },
        // The picker is not a registered field, so this one has nowhere to go
        // but the form-level slot; `reportFailure` puts it there.
        onError: (error) =>
          reportFailure(error.error, t('toasts.updatePlatformFailed')),
      },
    );
  };

  const handleRemoveProject = async () => {
    try {
      await remove.mutateAsync(project.id);
    } catch (error) {
      if (!(error instanceof ApiError)) throw error;
      // Toast only, deliberately. The Danger Zone sits outside the `<Form>`
      // wrapper, so a form-level message raised here renders up in Project
      // Details, nowhere near the button that was pressed.
      toast.error(t('toasts.deleteProjectFailed'), {
        description: describeError(error.error, formT),
      });
      return;
    }

    toast.success(t('toasts.deleted'));
    router.push('/projects');
  };

  /**
   * Put a failure where the user can act on it: on the input the server named,
   * or in the form-level slot plus a toast when it named nothing this form has.
   */
  function reportFailure(
    error: Parameters<typeof applyServerFieldErrors>[1],
    title: string,
  ) {
    const applied = applyServerFieldErrors(form, error, {
      labels: { name: t('fields.name'), slug: t('fields.slug') },
      t: formT,
    });

    if (applied.formLevel) {
      toast.error(title, { description: applied.formLevel });
    }
  }

  /**
   * Drop the form-level message once something has succeeded.
   *
   * `applyServerFieldErrors` clears this slot on its way in, so a *second*
   * failure replaces the first. A success calls nothing, and react-hook-form
   * only clears `root` inside `handleSubmit`, which this form never uses. So
   * without this the red "Failed to update platform" from an outage sits under
   * the section forever, next to a toast saying the platform was updated.
   */
  function clearFailure() {
    form.clearErrors(SERVER_ERROR_PATH);
  }

  /**
   * Drop a server-supplied error the moment the user edits the value it was
   * about.
   *
   * Each row saves on its own button rather than through `handleSubmit`, so
   * `formState.isSubmitted` never turns true and react-hook-form's
   * `reValidateMode: 'onChange'` never engages. Without this, "Project name is
   * already taken." stays under an input the user has already corrected, and
   * the only way to find out it is stale is to press Save again.
   */
  function clearServerError(field: 'name' | 'slug') {
    if (form.getFieldState(field).error?.type === 'server') {
      form.clearErrors(field);
    }
  }

  return (
    <div>
      <Form {...form}>
        <SettingSection title={t('projectDetails')}>
          <SavableRow
            form={form}
            name="name"
            title={t('nameTitle')}
            description={t('nameDescription')}
            placeholder={t('namePlaceholder')}
            isPending={isPending}
            readOnly={!canUpdate}
            hasChanges={hasNameChanges}
            onSave={handleSaveName}
            onEdit={() => clearServerError('name')}
          />

          <SavableRow
            form={form}
            name="slug"
            title={t('fields.slug')}
            description={t('slugDescription')}
            placeholder="project-slug"
            className="font-mono"
            isPending={isPending}
            readOnly={!canUpdate}
            hasChanges={hasSlugChanges}
            onSave={handleSaveSlug}
            onEdit={() => clearServerError('slug')}
          />

          <SettingRow
            title={t('fields.platform')}
            description={t('platformDescription')}
          >
            <PlatformPicker
              value={project.platform}
              onValueChange={handlePlatformChange}
              disabled={isPending || !canUpdate}
            />
          </SettingRow>

          {/* Where a failure that named no field of this form lands. */}
          <FormRootError />
        </SettingSection>
      </Form>

      {canDelete ? (
        <DangerZone project={project} onRemove={handleRemoveProject} />
      ) : null}
    </div>
  );
}
