import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { userQueries } from '@/features/user/api/queries';
import { MembersSettings } from '@/features/user/ui/components/members-settings';
import { session } from '@/shared/api/session';
import { PageLoading } from '@/shared/ui/components/page-loading';
import {
  SettingSection,
  SettingsListSkeleton,
} from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { useSessionUser } from '@/shared/ui/hooks/use-session-user';

export const Route = createFileRoute(
  '/_authenticated/projects/$id/settings/members',
)({
  loader: ({ context: { queryClient }, params }) => {
    void queryClient
      .query(userQueries.members(Number.parseInt(params.id, 10)))
      .catch(noop);
    // The instance's roster is who an admin can add; nobody else may read it.
    const answer = session.peek();
    if (answer?.state === 'authenticated' && answer.user.role === 'admin') {
      void queryClient.query(userQueries.team()).catch(noop);
    }
  },
  pendingComponent: MembersSettingsLoading,
  component: MembersSettingsPage,
});

function MembersSettingsPage() {
  const t = useTranslations('settings');
  const { id } = Route.useParams();
  const currentUser = useSessionUser();
  const projectId = Number.parseInt(id, 10);
  // Suspended on, not read beside the page: the members decide what the
  // reader may do here, so a failed read ends in the error boundary rather
  // than in an empty list beside a permission verdict computed from it.
  const { data: members } = useSuspenseQuery(userQueries.members(projectId));

  const currentMembership = members.find(
    (member) => member.user_id === currentUser.id,
  );
  const canManageMembers =
    currentUser.role === 'admin' || currentMembership?.role === 'admin';

  return (
    <>
      <title>{t('members.meta.title')}</title>
      <MembersSettings
        projectId={projectId}
        members={members}
        currentUserId={currentUser.id}
        canManage={canManageMembers}
      />
    </>
  );
}

function MembersSettingsLoading() {
  const t = useTranslations('settings');
  const tUser = useTranslations('user');
  return (
    <PageLoading>
      <title>{t('members.meta.title')}</title>
      {/* Its description and action depend on whether the reader may manage
          the members, which the members decide. */}
      <SettingsHeader title={tUser('members.title')} />
      <SettingSection title={tUser('membersList.title')}>
        <SettingsListSkeleton />
      </SettingSection>
    </PageLoading>
  );
}
