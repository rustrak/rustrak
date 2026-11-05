import { Button, Dialog, NewIcon, TeamIcon } from '@rustrak/ui';
import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useState } from 'react';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { userQueries } from '@/features/user/api/queries';
import { InviteForm } from '@/features/user/ui/components/invite-form';
import { PendingInvitations } from '@/features/user/ui/components/pending-invitations';
import { TeamMembersList } from '@/features/user/ui/components/team-members-list';
import { accessQueries, startIfAllowed } from '@/shared/api/permissions';
import { COMMAND_BAR_PROJECT_LIMIT } from '@/shared/config/commands';
import { PageLoading } from '@/shared/ui/components/page-loading';
import {
  SettingSection,
  SettingsEmpty,
  SettingsListSkeleton,
} from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { usePermissions } from '@/shared/ui/hooks/use-permissions';
import { useSessionUser } from '@/shared/ui/hooks/use-session-user';

export const Route = createFileRoute('/_authenticated/settings/team/')({
  /**
   * The roster is fetched only for an admin.
   *
   * Not an optimisation: a non-admin gets a 403 from both endpoints, and
   * loading them anyway would put "we could not load the team" underneath the
   * "you are not authorised" panel that already explains why.
   */
  loader: ({ context: { queryClient } }) => {
    startIfAllowed(
      queryClient,
      (p) => p.can('users', 'read'),
      () => void queryClient.query(userQueries.team()).catch(noop),
    );
    startIfAllowed(
      queryClient,
      (p) => p.can('invitations', 'read'),
      () => {
        void queryClient.query(userQueries.invitations()).catch(noop);
        void queryClient.query(userQueries.emailStatus()).catch(noop);
      },
    );
  },
  pendingComponent: TeamLoading,
  component: TeamPage,
});

function TeamPage() {
  const t = useTranslations('settings');
  const tUser = useTranslations('user');
  const user = useSessionUser();
  const permissions = usePermissions();

  const [inviting, setInviting] = useState(false);
  const canInvite = permissions.can('invitations', 'create');

  const heading = (
    <>
      <title>{t('team.meta.title')}</title>
      <SettingsHeader
        title={t('team.title')}
        description={t('team.subtitle')}
        actions={
          canInvite && permissions.can('users', 'read') ? (
            <Button
              variant="primary"
              icon={NewIcon}
              onClick={() => setInviting(true)}
            >
              {tUser('invite.submit')}
            </Button>
          ) : null
        }
      />
    </>
  );

  // Guard: only instance admins may manage the team. The "we could not ask"
  // branch that used to sit above this one belongs to `_authenticated` now —
  // an outage is not a permission verdict, and reporting one as the other is
  // the bug that branch exists to prevent.
  if (!permissions.can('users', 'read')) {
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
      <TeamRoster
        currentUserId={user.id}
        isOwner={permissions.isOwner}
        canDelete={permissions.can('users', 'delete')}
        canInvite={canInvite}
        inviting={inviting}
        onInvitingChange={setInviting}
        canSeeInvitations={permissions.can('invitations', 'read')}
        canReadRoles={permissions.can('roles', 'read')}
      />
    </>
  );
}

/** The roster: separate so its reads are only subscribed for who may read it. */
function TeamRoster({
  currentUserId,
  isOwner,
  canDelete,
  canInvite,
  canSeeInvitations,
  canReadRoles,
  inviting,
  onInvitingChange,
}: {
  inviting: boolean;
  onInvitingChange: (open: boolean) => void;
  currentUserId: number;
  isOwner: boolean;
  canDelete: boolean;
  canInvite: boolean;
  canSeeInvitations: boolean;
  canReadRoles: boolean;
}) {
  const { data: members } = useSuspenseQuery(userQueries.team());
  // Not suspended: an unreadable status reads as "cannot send", not as a
  // failed page.
  const { data: emailStatus } = useQuery({
    ...userQueries.emailStatus(),
    enabled: canSeeInvitations,
  });
  const emailAvailable = emailStatus?.available === true;
  const roles = useQuery({ ...userQueries.roles(), enabled: canReadRoles });
  const catalog = useQuery({ ...accessQueries.catalog(), enabled: inviting });
  const projects = useQuery({
    ...projectQueries.list({ per_page: COMMAND_BAR_PROJECT_LIMIT }),
    enabled: inviting,
  });

  return (
    <>
      <Dialog open={inviting} onOpenChange={onInvitingChange} size="sm">
        {canInvite && inviting ? (
          <InviteForm
            emailAvailable={emailAvailable}
            isOwner={isOwner}
            roles={roles.data ?? null}
            catalog={catalog.data?.resources ?? []}
            projects={projects.data?.items ?? []}
            onDone={() => onInvitingChange(false)}
          />
        ) : null}
      </Dialog>
      <TeamMembersList
        members={members}
        currentUserId={currentUserId}
        isOwner={isOwner}
        canDelete={canDelete}
      />
      {canSeeInvitations ? (
        <Invitations emailAvailable={emailAvailable} />
      ) : null}
    </>
  );
}

function Invitations({ emailAvailable }: { emailAvailable: boolean }) {
  const { data: invitations } = useSuspenseQuery(userQueries.invitations());
  return (
    <PendingInvitations
      invitations={invitations.filter((i) => i.status === 'pending')}
      emailAvailable={emailAvailable}
    />
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
        title={tUser('membersList.title')}
        description={tUser('membersList.description')}
      >
        <SettingsListSkeleton />
      </SettingSection>
    </PageLoading>
  );
}
