import { Skeleton } from '@rustrak/ui';
import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, notFound } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { userQueries } from '@/features/user/api/queries';
import { UserAssignments } from '@/features/user/ui/components/user-assignments';
import { accessQueries, startIfAllowed } from '@/shared/api/permissions';
import { COMMAND_BAR_PROJECT_LIMIT } from '@/shared/config/commands';
import { PageLoading } from '@/shared/ui/components/page-loading';
import {
  SettingSection,
  SettingsListSkeleton,
} from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { usePermissions } from '@/shared/ui/hooks/use-permissions';
import { useSessionUser } from '@/shared/ui/hooks/use-session-user';

// ponytail: the first 100 projects; add a search when someone has more.
const projectsToPick = projectQueries.list({
  per_page: COMMAND_BAR_PROJECT_LIMIT,
});

export const Route = createFileRoute('/_authenticated/settings/team/$userId')({
  loader: ({ context: { queryClient }, params }) => {
    const id = Number.parseInt(params.userId, 10);
    void queryClient.query(userQueries.team()).catch(noop);
    void queryClient.query(userQueries.assignments(id)).catch(noop);
    startIfAllowed(
      queryClient,
      (p) => p.can('roles', 'read'),
      () => void queryClient.query(userQueries.roles()).catch(noop),
    );
    void queryClient.query(accessQueries.catalog()).catch(noop);
    void queryClient.query(projectsToPick).catch(noop);
  },
  pendingComponent: UserLoading,
  component: UserPage,
});

function UserPage() {
  const t = useTranslations('access');
  const id = Number.parseInt(Route.useParams().userId, 10);
  const me = useSessionUser();
  const permissions = usePermissions();
  const { data: team } = useSuspenseQuery(userQueries.team());
  const { data: access } = useSuspenseQuery(userQueries.assignments(id));
  // Optional: what they hold names itself, and custom access needs no role.
  const { data: roles = [] } = useQuery({
    ...userQueries.roles(),
    enabled: permissions.can('roles', 'read'),
  });
  const { data: catalog } = useSuspenseQuery(accessQueries.catalog());
  const { data: projects } = useSuspenseQuery(projectsToPick);
  const user = team.find((u) => u.id === id);
  if (!user) throw notFound();

  const readOnlyReason =
    user.id === me.id
      ? t('users.readOnlySelf')
      : !permissions.can('users', 'update')
        ? t('users.readOnlyPermission')
        : null;

  return (
    <>
      <title>{t('users.meta', { email: user.email })}</title>
      <UserAssignments
        key={JSON.stringify(access)}
        user={user}
        access={access}
        roles={roles}
        catalog={catalog.resources}
        projects={projects.items}
        readOnlyReason={readOnlyReason}
      />
    </>
  );
}

function UserLoading() {
  const t = useTranslations('access');
  return (
    <PageLoading>
      <title>{t('meta.title')}</title>
      <SettingsHeader title={<Skeleton className="h-6 w-48" />} />
      <SettingSection title={t('users.accessTitle')}>
        <SettingsListSkeleton rows={2} />
      </SettingSection>
    </PageLoading>
  );
}
