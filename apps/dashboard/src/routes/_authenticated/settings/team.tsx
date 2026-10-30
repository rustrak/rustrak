import { Skeleton, TeamIcon } from '@rustrak/ui';
import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { userQueries } from '@/features/user/api/queries';
import { InviteForm } from '@/features/user/ui/components/invite-form';
import { PendingInvitations } from '@/features/user/ui/components/pending-invitations';
import { TeamMembersList } from '@/features/user/ui/components/team-members-list';
import { session } from '@/shared/api/session';
import { PageLoading } from '@/shared/ui/components/page-loading';
import {
  SettingSection,
  SettingsEmpty,
  SettingsListSkeleton,
} from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { useSessionUser } from '@/shared/ui/hooks/use-session-user';

export const Route = createFileRoute('/_authenticated/settings/team')({
  /**
   * The roster is fetched only for an admin.
   *
   * Not an optimisation: a non-admin gets a 403 from both endpoints, and
   * loading them anyway would put "we could not load the team" underneath the
   * "you are not authorised" panel that already explains why.
   */
  loader: ({ context: { queryClient } }) => {
    const answer = session.peek();
    if (answer?.state === 'authenticated' && answer.user.role === 'admin') {
      void queryClient.query(userQueries.team()).catch(noop);
      void queryClient.query(userQueries.invitations()).catch(noop);
      // Email delivery is a capability flag, not content: prefetch it so the
      // invite switch is correct on first paint, and let a failure fall
      // through — the component degrades to "cannot send" on its own.
      void queryClient.query(userQueries.emailStatus()).catch(noop);
    }
  },
  pendingComponent: TeamLoading,
  component: TeamPage,
});

function TeamPage() {
  const t = useTranslations('settings');
  const user = useSessionUser();

  const heading = (
    <>
      <title>{t('team.meta.title')}</title>
      <SettingsHeader
        title={t('team.title')}
        description={t('team.subtitle')}
      />
    </>
  );

  // Guard: only instance admins may manage the team. The "we could not ask"
  // branch that used to sit above this one belongs to `_authenticated` now —
  // an outage is not a permission verdict, and reporting one as the other is
  // the bug that branch exists to prevent.
  if (user.role !== 'admin') {
    return (
      <>
        {heading}
        <div className="overflow-hidden rounded-2xl border border-border-subtle bg-surface">
          <SettingsEmpty
            icon={<TeamIcon size="2xl" />}
            title={t('notAuthorized')}
            description={t('team.notAuthorizedDescription')}
          />
        </div>
      </>
    );
  }

  return (
    <>
      {heading}
      <TeamRoster currentUserId={user.id} />
    </>
  );
}

/** The admin's view: separate so its reads are only subscribed for an admin. */
function TeamRoster({ currentUserId }: { currentUserId: number }) {
  const { data: members } = useSuspenseQuery(userQueries.team());
  const { data: invitations } = useSuspenseQuery(userQueries.invitations());
  // Suspense is deliberately not used here: an unreadable status means
  // "assume delivery is off" (the switch greys out), it must never take the
  // whole team page down with it.
  const { data: emailStatus } = useQuery(userQueries.emailStatus());
  const emailAvailable = emailStatus?.available === true;

  const pendingInvitations = invitations.filter(
    (invitation) => invitation.status === 'pending',
  );

  return (
    <>
      <InviteForm emailAvailable={emailAvailable} />
      <TeamMembersList members={members} currentUserId={currentUserId} />
      <PendingInvitations
        invitations={pendingInvitations}
        emailAvailable={emailAvailable}
      />
    </>
  );
}

/** Only an admin waits here: nobody else reads anything. */
function TeamLoading() {
  const t = useTranslations('settings');
  const tUser = useTranslations('user');
  return (
    <PageLoading>
      <title>{t('team.meta.title')}</title>
      <SettingsHeader
        title={t('team.title')}
        description={t('team.subtitle')}
      />
      <SettingSection
        title={tUser('invite.title')}
        description={tUser('invite.description')}
      >
        <div className="px-5 py-4">
          <Skeleton className="h-9 w-full rounded-lg" />
        </div>
      </SettingSection>
      <SettingSection
        title={tUser('membersList.title')}
        description={tUser('membersList.description')}
      >
        <SettingsListSkeleton />
      </SettingSection>
    </PageLoading>
  );
}
