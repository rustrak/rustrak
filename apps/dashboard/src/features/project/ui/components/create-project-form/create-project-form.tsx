import { zodResolver } from '@hookform/resolvers/zod';
import { useNavigate } from '@tanstack/react-router';
import { Loader2 } from 'lucide-react';
import { PlatformIcon } from 'platformicons';
import { useMemo, useTransition } from 'react';
import { useForm } from 'react-hook-form';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { z } from 'zod';
import { createProject } from '@/features/project/api/mutations';
import {
  projectNameField,
  projectSlugField,
} from '@/features/project/model/fields';
import { useProjectSlug } from '@/features/project/ui/hooks/use-project-slug';
import { platformLabel } from '@/shared/config/platforms';
import type { Translate } from '@/shared/lib/error-copy';
import { applyServerFieldErrors } from '@/shared/lib/form-errors';
import { PlatformGrid } from '@/shared/ui/components/platform-grid';
import { Button } from '@/shared/ui/components/shadcn/button';
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  FormRootError,
} from '@/shared/ui/components/shadcn/form';
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
 * Platform grid plus name field, submitting to `createProject`.
 *
 * `existingNames` only feeds the suggested default name; the server still has
 * the final say on collisions.
 */
export function CreateProjectForm({ existingNames }: CreateProjectFormProps) {
  const t = useTranslations('projects');
  const formT = useTranslations();
  const navigate = useNavigate();
  const [isPending, startTransition] = useTransition();

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
    startTransition(async () => {
      const result = await createProject({
        name: data.name,
        platform: data.platform,
        // Only sent when the user took the field over. An auto slug is
        // derived, and the server is allowed to de-duplicate a derived slug
        // silently. Sending the preview would turn that into a 409.
        ...(slug.isManual ? { slug: data.slug } : {}),
      });

      if (!result.success) {
        // A taken name or slug belongs on its own field, not in a toast the
        // user has to translate back into an edit. The server names the
        // offending input as data now (`fields: [{field: 'slug', code:
        // 'already_exists'}]`), so nothing here reads the message prose. The
        // helper only marks a field this form registers, so a name the form
        // does not have cannot strand it.
        const applied = applyServerFieldErrors(form, result.error, {
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
        return;
      }

      // Straight into Client Keys: the DSN and the platform's setup snippet
      // are the only thing left to do, and that page owns them.
      navigate({
        to: '/projects/$id/settings/client-keys',
        params: { id: result.data.id },
      });
    });
  };

  // Rendered in two places (the aside on desktop, the fixed bar on mobile), so
  // it lives here rather than being written twice.
  const submitLabel = isPending ? (
    <>
      <Loader2 className="mr-2 size-4 animate-spin" />
      {t('creating')}
    </>
  ) : (
    t('create')
  );

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
              <FormLabel className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
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

        <aside className="rounded-xl border bg-card p-5 lg:sticky lg:top-6">
          <div className="flex items-center gap-3 border-b pb-4">
            {platform ? (
              <>
                <PlatformIcon platform={platform} size={32} />
                <div className="min-w-0">
                  <p className="truncate text-sm font-bold">
                    {platformLabel(platform)}
                  </p>
                  <p className="truncate font-mono text-xs text-muted-foreground">
                    {platform}
                  </p>
                </div>
              </>
            ) : (
              <>
                <div className="size-8 shrink-0 rounded-md border border-dashed" />
                <p className="text-sm text-muted-foreground">
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
            disabled={isPending}
            className="mt-5 hidden w-full lg:flex"
          >
            {submitLabel}
          </Button>

          <p className="mt-3 hidden text-xs text-muted-foreground lg:block">
            {t('dsnNext')}
          </p>
        </aside>

        {/* Mobile action bar. The platform grid is tall, so a button sitting
            below it is off-screen for most of the time the user spends on this
            page. Pinning it also keeps the current selection visible while
            scrolling the list. */}
        <div className="fixed inset-x-0 bottom-0 z-40 border-t bg-background/95 px-4 py-3 backdrop-blur lg:hidden">
          <div className="mx-auto flex max-w-6xl items-center gap-3">
            {platform ? (
              <>
                <PlatformIcon platform={platform} size={24} />
                <span className="min-w-0 flex-1 truncate text-sm font-medium">
                  {platformLabel(platform)}
                </span>
              </>
            ) : (
              <span className="min-w-0 flex-1 truncate text-sm text-muted-foreground">
                {t('pickPlatform')}
              </span>
            )}
            <Button
              type="submit"
              disabled={isPending}
              className="shrink-0"
              size="sm"
            >
              {submitLabel}
            </Button>
          </div>
        </div>
      </form>
    </Form>
  );
}
