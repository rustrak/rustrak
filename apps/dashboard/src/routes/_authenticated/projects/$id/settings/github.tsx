import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { githubQueries } from '@/features/github/api/queries';
import { RepositorySettings } from '@/features/github/ui/components/repository-settings';
import { projectQueries } from '@/features/project/api/queries';
import { startIfAllowed } from '@/shared/api/permissions';
import { NotAuthorized } from '@/shared/ui/components/not-authorized';
import { PageLoading } from '@/shared/ui/components/page-loading';
import {
  SettingSection,
  SettingsListSkeleton,
} from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { usePermissions } from '@/shared/ui/hooks/use-permissions';

export const Route = createFileRoute(
  '/_authenticated/projects/$id/settings/github',
)({
  loader: ({ context: { queryClient }, params }) => {
    const projectId = Number.parseInt(params.id, 10);
    // The picker's choices fill in on their own; their failure is how the
    // page says GitHub is not connected yet.
    startIfAllowed(
      queryClient,
      (p) => p.can('project-github', 'read', projectId),
      () =>
        void queryClient.query(githubQueries.repository(projectId)).catch(noop),
    );
    startIfAllowed(
      queryClient,
      (p) => p.can('project-github', 'update', projectId),
      () =>
        void queryClient
          .query(githubQueries.repositories(projectId))
          .catch(noop),
    );
  },
  pendingComponent: GitHubSettingsLoading,
  component: GitHubSettingsPage,
});

function GitHubSettingsPage() {
  const t = useTranslations('github');
  const projectId = Number.parseInt(Route.useParams().id, 10);
  const canRead = usePermissions().can('project-github', 'read', projectId);

  return (
    <>
      <title>{t('repository.metaTitle')}</title>
      <SettingsHeader title="GitHub" description={t('repository.subtitle')} />
      {canRead ? <Repository projectId={projectId} /> : <NotAuthorized />}
    </>
  );
}

/** Separate so the repository is only read by who may read it. */
function Repository({ projectId }: { projectId: number }) {
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  const { data: repository } = useSuspenseQuery(
    githubQueries.repository(projectId),
  );
  const canUpdate = usePermissions().can('project-github', 'update', projectId);

  return (
    <RepositorySettings
      readOnly={!canUpdate}
      // A new repository starts from its own saved mappings.
      key={repository?.repo_full_name ?? 'none'}
      projectId={project.id}
      projectSlug={project.slug}
      repository={repository}
    />
  );
}

function GitHubSettingsLoading() {
  const t = useTranslations('github');
  return (
    <PageLoading>
      <title>{t('repository.metaTitle')}</title>
      <SettingsHeader title="GitHub" description={t('repository.subtitle')} />
      <SettingSection title={t('repository.section')}>
        <SettingsListSkeleton rows={1} />
      </SettingSection>
    </PageLoading>
  );
}
