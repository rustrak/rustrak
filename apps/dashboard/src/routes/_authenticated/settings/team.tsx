import { TeamIcon } from '@rustrak/ui';
import { useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { userQueries } from '@/features/user/api/queries';
import { InviteForm } from '@/features/user/ui/components/invite-form';
import { PendingInvitations } from '@/features/user/ui/components/pending-invitations';
import { TeamMembersList } from '@/features/user/ui/components/team-members-list';
import { session } from '@/shared/api/session';
import { translator } from '@/shared/i18n/intl';
import { SettingsEmpty } from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { useSessionUser } from '@/shared/ui/hooks/use-session-user';

export const Route = createFileRoute('/_authenticated/settings/team')({
  head: () => {
    const t = translator('settings');
    return {
      meta: [
        { title: t('team.meta.title') },
        { name: 'description', content: t('team.meta.description') },
      ],
    };
  },
  /**
   * The roster is fetched only for an admin.
   *
   * Not an optimisation: a non-admin gets a 403 from both endpoints, and
   * loading them anyway would put "we could not load the team" underneath the
   * "you are not authorised" panel that already explains why.
   */
  loader: async ({ context: { queryClient } }) => {
    const answer = session.peek();
    if (answer?.state !== 'authenticated' || answer.user.role !== 'admin') {
      return;
    }
    await Promise.all([
      queryClient.query({ ...userQueries.team(), staleTime: 'static' }),
      queryClient.query({ ...userQueries.invitations(), staleTime: 'static' }),
    ]);
    // Email capability only decides one switch's enabled state, so a failed
    // read is caught here: the page renders with the switch disabled instead
    // of failing the route over a status probe.
    try {
      await queryClient.query({
        ...userQueries.emailStatus(),
        staleTime: 'static',
      });
    } catch {
      // Degrade below: the component reads "unavailable" from the miss.
    }
  },
  component: TeamPage,
});

function TeamPage() {
  const t = useTranslations('settings');
  const user = useSessionUser();

  const heading = (
    <SettingsHeader title={t('team.title')} description={t('team.subtitle')} />
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
  // Not a suspense read on purpose: the loader usually has this cached, but
  // when the probe failed the page must still render — `undefined` reads as
  // "cannot send email" and the invite form keeps its switch disabled.
  const { data: emailStatus } = useQuery(userQueries.emailStatus());

  const pendingInvitations = invitations.filter(
    (invitation) => invitation.status === 'pending',
  );

  return (
    <>
      <InviteForm emailAvailable={emailStatus?.available ?? false} />
      <TeamMembersList members={members} currentUserId={currentUserId} />
      <PendingInvitations invitations={pendingInvitations} />
    </>
  );
}
