import { zodResolver } from '@hookform/resolvers/zod';
import { Button } from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { PlatformIcon } from 'platformicons';
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
import { useProjectSlug } from '@/features/project/ui/hooks/use-project-slug';
import { platformLabel } from '@/shared/config/platforms';
import type { Translate } from '@/shared/lib/error-copy';
import { applyServerFieldErrors } from '@/shared/lib/form-errors';
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  FormRootError,
} from '@/shared/ui/components/form';
import { PlatformGrid } from '@/shared/ui/components/platform-grid';
import { useRouter } from '@/shared/ui/hooks/use-router';
import { IdentityFields } from './identity-fields';

function buildCreateProjectFormSchema(t: Translate) {
  return z.object({
    platform: z.string().min(1, t('platformRequired')),
    name: projectNameField(t),
    slug: projectSlugField(t),
  });
}

export type CreateProjectFormData = z.infer<
  ReturnType<typeof buildCreateProjectFormSchema>
>;

interface CreateProjectFormProps {
  /**
   * Names already in use, so the auto-filled suggestion can avoid one.
   *
   * Best-effort only: it is one page of projects, not the whole set, and
   * another tab can take a name in between. The server is the real authority
   * and answers with a 409, which is surfaced as a toast.
   */
  existingNames: string[];
}

/**
 * Suggests a project name for a platform, avoiding names already taken.
 *
 * Sentry pre-fills the name field with the raw platform id, which works there
 * because `name` is unconstrained and only `slug` is de-duplicated. Rustrak
 * has a UNIQUE constraint on `projects.name`, so a second Next.js project
 * would collide on the most common path there is. Suffix instead.
 */
function suggestName(platformId: string, taken: Set<string>): string {
  if (!taken.has(platformId)) return platformId;
  for (let n = 2; n < 1000; n++) {
    const candidate = `${platformId}-${n}`;
    if (!taken.has(candidate)) return candidate;
  }
  return platformId;
}

/**
 * Platform grid plus name field, submitting to `projectMutations.create`.
 *
 * `existingNames` only feeds the suggested default name; the server still has
 * the final say on collisions.
 */
export function CreateProjectForm({ existingNames }: CreateProjectFormProps) {
  const t = useTranslations('projects');
  const formT = useTranslations();
  const router = useRouter();
  const create = useMutation(projectMutations.create());
  const isPending = create.isPending;

  const createProjectFormSchema = useMemo(
    () => buildCreateProjectFormSchema(t),
    [t],
  );

  const form = useForm<CreateProjectFormData>({
    resolver: zodResolver(createProjectFormSchema),
    defaultValues: { platform: '', name: '', slug: '' },
  });

  const slug = useProjectSlug(form);
  const platform = form.watch('platform');
  const taken = new Set(existingNames);

  // The slug is generated from the name until the user explicitly takes it
  // over with the Edit button. An explicit mode rather than a dirty-field
  // heuristic, because the field is what the button controls: read-only while
  // it mirrors the name, editable once it stops.
  // In a handler rather than an effect: this is a reaction to the user typing,
  // not to a render.
  const handlePlatformChange = (platformId: string) => {
    form.setValue('platform', platformId, { shouldValidate: true });

    // Only auto-fill while the user has not taken over the field, matching
    // Sentry's `hasUserModifiedProjectName` guard. `shouldDirty: false` is
    // what makes `dirtyFields.name` mean "the user typed", not "we filled it".
    if (!form.formState.dirtyFields.name) {
      const suggested = suggestName(platformId, taken);
      form.setValue('name', suggested, {
        shouldDirty: false,
        shouldValidate: true,
      });
      slug.syncFromName(suggested);
    }
  };

  const onSubmit = (data: CreateProjectFormData) => {
    create.mutate(
      {
        name: data.name,
        platform: data.platform,
        // Only sent when the user took the field over. An auto slug is
        // derived, and the server is allowed to de-duplicate a derived slug
        // silently. Sending the preview would turn that into a 409.
        ...(slug.isManual ? { slug: data.slug } : {}),
      },
      {
        // Straight into Client Keys: the DSN and the platform's setup snippet
        // are the only thing left to do, and that page owns them.
        onSuccess: (project) =>
          router.push(`/projects/${project.id}/settings/client-keys`),
        onError: (error) => {
          // A taken name or slug belongs on its own field, not in a toast the
          // user has to translate back into an edit. The server names the
          // offending input as data now (`fields: [{field: 'slug', code:
          // 'already_exists'}]`), so nothing here reads the message prose. The
          // helper only marks a field this form registers, so a name the form
          // does not have cannot strand it.
          const applied = applyServerFieldErrors(form, error.error, {
            labels: { name: t('fields.name'), slug: t('fields.slug') },
            t: formT,
          });

          // The slug input is read-only while it mirrors the name, so a slug the
          // server rejected would be marked and unfixable. Hand the field over.
          if (applied.marked.includes('slug')) {
            slug.takeOver();
          }

          // Only when nothing landed on an input: a toast next to a red field is
          // the same sentence twice, and it outlives the fix.
          if (applied.formLevel) {
            toast.error(t('toasts.createFailed'), {
              description: applied.formLevel,
            });
          }
        },
      },
    );
  };

  // Rendered in two places (the aside on desktop, the fixed bar on mobile), so
  // it lives here rather than being written twice.
  const submitLabel = isPending ? t('creating') : t('create');

  return (
    <Form {...form}>
      <form
        onSubmit={form.handleSubmit(onSubmit)}
        // Bottom padding clears the fixed mobile bar, which would otherwise
        // cover the last field.
        className="grid gap-6 pb-24 lg:grid-cols-[minmax(0,1fr)_20rem] lg:items-start lg:pb-0"
      >
        <FormField
          control={form.control}
          name="platform"
          render={({ field }) => (
            <FormItem className="min-w-0">
              <FormLabel className="text-label text-fg-muted">
                {t('fields.platform')}
              </FormLabel>
              <FormControl>
                <PlatformGrid
                  value={field.value || null}
                  onValueChange={handlePlatformChange}
                  disabled={isPending}
                />
              </FormControl>
              <FormMessage />
            </FormItem>
          )}
        />

        {/* Labelled like the platform column, with the same gap, so both
            panels start on the same line. */}
        <div className="grid gap-2 lg:sticky lg:top-0">
          <p className="text-label text-fg-muted">{t('projectDetails')}</p>
          <aside className="rounded-2xl border border-border-subtle bg-surface p-5">
            <div className="flex items-center gap-3 border-b border-border-divider pb-4">
              {platform ? (
                // Keyed by the platform, so picking another replays the swap.
                <div
                  key={platform}
                  className="flex min-w-0 items-center gap-3 animate-swap-in"
                >
                  <PlatformIcon
                    platform={platform}
                    size={32}
                    className="shrink-0 rounded-lg"
                  />
                  <div className="min-w-0">
                    <p className="truncate text-card-title text-fg">
                      {platformLabel(platform)}
                    </p>
                    <p className="truncate font-mono text-mono-sm text-fg-meta">
                      {platform}
                    </p>
                  </div>
                </div>
              ) : (
                <>
                  <div className="size-8 shrink-0 rounded-lg border border-dashed border-border-strong" />
                  <p className="text-value text-fg-subtle">
                    {t('noPlatformSelected')}
                  </p>
                </>
              )}
            </div>

            <IdentityFields slug={slug} disabled={isPending} />

            {/* Where a server failure that named no field of this form lands. */}
            <FormRootError className="mt-4" />

            {/* Hidden on mobile: the fixed bar below owns the action there, and
              two live submit buttons would be one too many. */}
            <Button
              type="submit"
              variant="primary"
              loading={isPending}
              disabled={isPending}
              className="mt-5 hidden w-full lg:flex"
            >
              {submitLabel}
            </Button>

            <p className="mt-3 hidden text-hint text-fg-meta lg:block">
              {t('dsnNext')}
            </p>
          </aside>
        </div>

        {/* Mobile action bar. The platform grid is tall, so a button sitting
            below it is off-screen for most of the time the user spends on this
            page. Pinning it also keeps the current selection visible while
            scrolling the list. */}
        <div className="fixed inset-x-0 bottom-0 z-40 border-t border-border-divider bg-panel px-4 py-3 lg:hidden">
          <div className="mx-auto flex max-w-6xl items-center gap-3">
            {platform ? (
              <>
                <PlatformIcon
                  platform={platform}
                  size={24}
                  className="shrink-0 rounded-md"
                />
                <span className="min-w-0 flex-1 truncate text-value font-medium text-fg">
                  {platformLabel(platform)}
                </span>
              </>
            ) : (
              <span className="min-w-0 flex-1 truncate text-value text-fg-subtle">
                {t('pickPlatform')}
              </span>
            )}
            <Button
              type="submit"
              variant="primary"
              size="sm"
              loading={isPending}
              disabled={isPending}
              className="shrink-0"
            >
              {submitLabel}
            </Button>
          </div>
        </div>
      </form>
    </Form>
  );
}
