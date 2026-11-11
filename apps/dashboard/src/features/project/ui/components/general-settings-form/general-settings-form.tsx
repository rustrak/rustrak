import type { ProjectResponse } from '@rustrak/client';
import { useSelector } from '@tanstack/react-form';
import { useMutation } from '@tanstack/react-query';
import { useMemo, useRef } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { z } from 'zod';
import { projectMutations } from '@/features/project/api/mutations';
import {
  projectNameField,
  projectSlugField,
} from '@/features/project/model/fields';
import type { Translate } from '@/shared/lib/error-copy';
import { applyServerFieldErrors } from '@/shared/lib/form-errors';
import { useAppForm } from '@/shared/ui/components/app-form';
import { PlatformPicker } from '@/shared/ui/components/platform-picker';
import { SettingRow, SettingSection } from '@/shared/ui/components/setting-row';
import { useRouter } from '@/shared/ui/hooks/use-router';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';
import { DangerZone } from './danger-zone';
import { SavableRow } from './savable-row';

/** The same rules the create form uses, imported rather than restated. */
function buildGeneralSettingsSchema(t: Translate) {
  return z.object({
    name: projectNameField(t),
    slug: projectSlugField(t),
  });
}

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
  const toastFailure = useToastFailure();
  const router = useRouter();
  const update = useMutation(projectMutations.update(project.id));
  const remove = useMutation(projectMutations.delete());
  const isPending = update.isPending;

  const generalSettingsSchema = useMemo(
    () => buildGeneralSettingsSchema(t),
    [t],
  );

  // Each row saves on its own, so each field carries its own rule: a
  // form-level schema would mark the slug red when only the name was saved.
  const form = useAppForm({
    defaultValues: { name: project.name, slug: project.slug },
  });
  // Fields whose current error came from the server, not the schema.
  const serverMarked = useRef(new Set<string>());

  const name = useSelector(form.store, (state) => state.values.name);
  const slug = useSelector(form.store, (state) => state.values.slug);
  const hasNameChanges = name !== project.name;
  const hasSlugChanges = slug !== project.slug;

  const handleSaveName = () => {
    void Promise.resolve(form.validateField('name', 'submit')).then(
      (errors) => {
        if (errors.length > 0 || !hasNameChanges) return;

        const trimmed = form.getFieldValue('name').trim();
        update.mutate(
          { name: trimmed },
          {
            onSuccess: () => {
              // Keep the field on the value that was actually persisted,
              // otherwise stray whitespace leaves the Save button looking
              // permanently dirty.
              form.setFieldValue('name', trimmed);
              clearFailure();
              toast.success(t('toasts.nameUpdated'));
            },
            onError: (error) =>
              reportFailure(error.error, t('toasts.updateNameFailed')),
          },
        );
      },
    );
  };

  const handleSaveSlug = () => {
    void Promise.resolve(form.validateField('slug', 'submit')).then(
      (errors) => {
        if (errors.length > 0 || !hasSlugChanges) return;

        update.mutate(
          { slug: form.getFieldValue('slug').trim() },
          {
            onSuccess: (updated) => {
              // The server slugifies the input, so echo back what it actually
              // stored rather than the raw text: typing "My API" must leave the
              // field showing "my-api", not a value that was never persisted.
              form.setFieldValue('slug', updated.slug);
              clearFailure();
              toast.success(t('toasts.slugUpdated'));
            },
            onError: (error) =>
              reportFailure(error.error, t('toasts.updateSlugFailed')),
          },
        );
      },
    );
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
        // The picker is not a form field, so this one has nowhere to go but
        // the form-level slot; `reportFailure` puts it there.
        onError: (error) =>
          reportFailure(error.error, t('toasts.updatePlatformFailed')),
      },
    );
  };

  const handleRemoveProject = () =>
    remove.mutate(project.id, {
      // Toast only, deliberately. The Danger Zone sits outside the form's
      // section, so a form-level message raised here renders up in Project
      // Details, nowhere near the button that was pressed.
      onError: (error) => toastFailure(t('toasts.deleteProjectFailed'), error),
      onSuccess: () => {
        toast.success(t('toasts.deleted', { count: 1 }));
        router.push('/projects');
      },
    });

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
    serverMarked.current = new Set(applied.marked);

    if (applied.formLevel) {
      toast.error(title, { description: applied.formLevel });
    }
  }

  /**
   * Drop the form-level message once something has succeeded. A success
   * calls nothing else, so without this the red "Failed to update platform"
   * from an outage sits under the section next to a toast saying the platform
   * was updated.
   */
  function clearFailure() {
    form.setErrorMap({ onDynamic: undefined });
  }

  /**
   * Drop a server-supplied error the moment the user edits the value it was
   * about. Each row saves on its own rather than through a submit, so the
   * schema never re-runs on change; without this, "Project name is already
   * taken." stays under an input the user has already corrected.
   */
  function clearServerError(field: 'name' | 'slug') {
    if (!serverMarked.current.delete(field)) return;
    form.setFieldMeta(field, (meta) => ({
      ...meta,
      errorMap: { ...meta.errorMap, onDynamic: undefined },
    }));
  }

  return (
    <div>
      <form.AppForm>
        <SettingSection title={t('projectDetails')}>
          <form.AppField
            name="name"
            validators={{ onSubmit: generalSettingsSchema.shape.name }}
          >
            {() => (
              <SavableRow
                title={t('nameTitle')}
                description={t('nameDescription')}
                placeholder={t('namePlaceholder')}
                isPending={isPending}
                readOnly={!canUpdate}
                hasChanges={hasNameChanges}
                onSave={handleSaveName}
                onEdit={() => clearServerError('name')}
              />
            )}
          </form.AppField>

          <form.AppField
            name="slug"
            validators={{ onSubmit: generalSettingsSchema.shape.slug }}
          >
            {() => (
              <SavableRow
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
            )}
          </form.AppField>

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

          <form.RootError />
        </SettingSection>
      </form.AppForm>

      {canDelete ? (
        <DangerZone
          project={project}
          onRemove={handleRemoveProject}
          isPending={remove.isPending}
        />
      ) : null}
    </div>
  );
}
