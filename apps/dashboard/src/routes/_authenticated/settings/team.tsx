import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { ShieldX } from 'lucide-react';
import { useTranslations } from 'use-intl';
import { userQueries } from '@/features/user/api/queries';
import { InviteForm } from '@/features/user/ui/components/invite-form';
import { PendingInvitations } from '@/features/user/ui/components/pending-invitations';
import { TeamMembersList } from '@/features/user/ui/components/team-members-list';
import { session } from '@/shared/api/session';
import { translator } from '@/shared/i18n/intl';
import { combine, loadAll } from '@/shared/lib/results';
import { LoadFailure } from '@/shared/ui/components/load-failure';
import { Card, CardContent } from '@/shared/ui/components/shadcn/card';
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
    await loadAll([
      queryClient.ensureQueryData(userQueries.team()),
      queryClient.ensureQueryData(userQueries.invitations()),
    ]);
  },
  component: TeamPage,
});

function TeamPage() {
  const t = useTranslations('settings');
  const user = useSessionUser();

  const heading = (
    <div className="mb-6 md:mb-8">
      <h1 className="text-xl md:text-2xl font-extrabold tracking-tight">
        {t('team.title')}
      </h1>
      <p className="text-muted-foreground mt-1">{t('team.subtitle')}</p>
    </div>
  );

  // Guard: only instance admins may manage the team. The "we could not ask"
  // branch that used to sit above this one belongs to `_authenticated` now —
  // an outage is not a permission verdict, and reporting one as the other is
  // the bug that branch exists to prevent.
  if (user.role !== 'admin') {
    return (
      <>
        {heading}
        <Card className="border-dashed">
          <CardContent className="flex flex-col items-center justify-center py-12 text-center">
            <ShieldX className="size-12 text-muted-foreground/50 mb-4" />
            <p className="font-semibold">{t('notAuthorized')}</p>
            <p className="text-muted-foreground mt-1 text-sm max-w-sm">
              {t('team.notAuthorizedDescription')}
            </p>
          </CardContent>
        </Card>
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

function TeamRoster({ currentUserId }: { currentUserId: number }) {
  const t = useTranslations('settings');
  const loaded = combine([
    useSuspenseQuery(userQueries.team()).data,
    useSuspenseQuery(userQueries.invitations()).data,
  ]);

  if (!loaded.success) {
    return (
      <LoadFailure
        error={loaded.error}
        title={t('team.loadFailed')}
        notFoundOnMissing={false}
      />
    );
  }

  const [members, invitations] = loaded.data;

  const pendingInvitations = invitations.filter(
    (invitation) => invitation.status === 'pending',
  );

  return (
    <div className="space-y-6">
      <InviteForm />
      <TeamMembersList members={members} currentUserId={currentUserId} />
      <PendingInvitations invitations={pendingInvitations} />
    </div>
  );
}
