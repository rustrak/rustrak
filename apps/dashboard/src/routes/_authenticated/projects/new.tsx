import { Page, PageHeader, Skeleton } from '@rustrak/ui';
import { noop, useQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { CreateProjectForm } from '@/features/project/ui/components/create-project-form/create-project-form';
import { accessQueries } from '@/shared/api/permissions';
import { NotAuthorized } from '@/shared/ui/components/not-authorized';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { usePermissions } from '@/shared/ui/hooks/use-permissions';

/**
 * Create-project page: platform picker plus name, in one form.
 *
 * A full page rather than a dialog, matching Sentry's own `/projects/new/`.
 */
export const Route = createFileRoute('/_authenticated/projects/new')({
  // Only used to suggest a free default name. `projects.name` is UNIQUE, so
  // the raw platform id Sentry pre-fills would collide on a second Next.js
  // project. One page is enough for a suggestion; the server still has the
  // final say and returns a 409 on a genuine collision.
  //
  // A failed lookup only costs a less clever default name, so it must not block
  // project creation: the page reads it with `useQuery`, never suspends on it,
  // and suggests from no names at all when it fails.
  loader: ({ context: { queryClient } }) => {
    void queryClient.query(existingProjects).catch(noop);
    void queryClient.query(accessQueries.me()).catch(noop);
  },
  pendingComponent: NewProjectLoading,
  component: NewProjectPage,
});

const existingProjects = projectQueries.picker();

function NewProjectPage() {
  const t = useTranslations('projectPages');
  const canCreate = usePermissions().canCreateProject;
  const existing = useQuery({
    ...existingProjects,
    select: (page) => page.items.map((p) => p.name),
  });

  return (
    <div className="flex h-full flex-col">
      <title>{t('newProject.meta.title')}</title>
      <PageHeader
        title={t('newProject.title')}
        summary={[t('newProject.subtitle')]}
      />
      <Page>
        {canCreate ? (
          <CreateProjectForm existingNames={existing.data ?? []} />
        ) : (
          <NotAuthorized scope="workspace" />
        )}
      </Page>
    </div>
  );
}

function NewProjectLoading() {
  const t = useTranslations('projectPages');
  return (
    <PageLoading>
      <div className="flex h-full flex-col">
        <title>{t('newProject.meta.title')}</title>
        <PageHeader
          title={t('newProject.title')}
          summary={[t('newProject.subtitle')]}
        />
        <Page>
          <Skeleton className="h-64 w-full rounded-2xl" />
        </Page>
      </div>
    </PageLoading>
  );
}
