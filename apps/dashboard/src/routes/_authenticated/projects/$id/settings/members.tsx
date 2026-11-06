import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { userQueries } from '@/features/user/api/queries';
import { ProjectMembersList } from '@/features/user/ui/components/project-members-list';
import { startIfAllowed } from '@/shared/api/permissions';
import { NotAuthorized } from '@/shared/ui/components/not-authorized';
import { PageLoading } from '@/shared/ui/components/page-loading';
import {
  SettingSection,
  SettingsListSkeleton,
} from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { usePermissions } from '@/shared/ui/hooks/use-permissions';
import { useSessionUser } from '@/shared/ui/hooks/use-session-user';

/** Who holds a role on this project, for who manages it without the team page. */
export const Route = createFileRoute(
  '/_authenticated/projects/$id/settings/members',
)({
  loader: ({ context: { queryClient }, params }) => {
    const projectId = Number.parseInt(params.id, 10);
    startIfAllowed(
      queryClient,
      (p) => p.can('project-members', 'read', projectId),
      () => void queryClient.query(userQueries.members(projectId)).catch(noop),
    );
    startIfAllowed(
      queryClient,
      (p) => p.can('project-members', 'update', projectId),
      () =>
        void queryClient
          .query(userQueries.memberCandidates(projectId))
          .catch(noop),
    );
  },
  pendingComponent: MembersLoading,
  component: MembersPage,
});

function MembersPage() {
  const t = useTranslations('settings');
  const tUser = useTranslations('user');
  const projectId = Number.parseInt(Route.useParams().id, 10);
  const canRead = usePermissions().can('project-members', 'read', projectId);
  if (canRead) {
    return <Members projectId={projectId} />;
  }
  return (
    <>
      <title>{t('members.meta.title')}</title>
      <SettingsHeader title={tUser('members.title')} />
      <NotAuthorized />
    </>
  );
}

/** Separate so the members are only read by who may read them. */
function Members({ projectId }: { projectId: number }) {
  const t = useTranslations('settings');
  const user = useSessionUser();
  const canUpdate = usePermissions().can(
    'project-members',
    'update',
    projectId,
  );
  const { data: members } = useSuspenseQuery(userQueries.members(projectId));
  // Not suspended: without them the list still stands, and can still remove.
  const { data: candidates } = useQuery({
    ...userQueries.memberCandidates(projectId),
    enabled: canUpdate,
  });
  return (
    <>
      <title>{t('members.meta.title')}</title>
      <ProjectMembersList
        projectId={projectId}
        members={members}
        candidates={candidates ?? null}
        canUpdate={canUpdate}
        currentUserId={user.id}
      />
    </>
  );
}

function MembersLoading() {
  const t = useTranslations('settings');
  const tUser = useTranslations('user');
  return (
    <PageLoading>
      <title>{t('members.meta.title')}</title>
      <SettingsHeader title={tUser('members.title')} />
      <SettingSection title={tUser('members.title')}>
        <SettingsListSkeleton />
      </SettingSection>
    </PageLoading>
  );
}
