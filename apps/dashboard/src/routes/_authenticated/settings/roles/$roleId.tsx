import { Skeleton } from '@rustrak/ui';
import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, Navigate } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { userQueries } from '@/features/user/api/queries';
import { RoleDocument } from '@/features/user/ui/components/role-document';
import { accessQueries, startIfAllowed } from '@/shared/api/permissions';
import { NotAuthorized } from '@/shared/ui/components/not-authorized';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { usePermissions } from '@/shared/ui/hooks/use-permissions';

export const Route = createFileRoute('/_authenticated/settings/roles/$roleId')({
  loader: ({ context: { queryClient } }) => {
    startIfAllowed(
      queryClient,
      (p) => p.can('roles', 'read'),
      () => {
        void queryClient.query(userQueries.roles()).catch(noop);
        void queryClient.query(accessQueries.catalog()).catch(noop);
      },
    );
  },
  pendingComponent: RoleLoading,
  component: RolePage,
});

function RolePage() {
  return usePermissions().can('roles', 'read') ? (
    <Role />
  ) : (
    <NotAuthorized scope="workspace" />
  );
}

/** Separate so the roles are only read by who may read them. */
function Role() {
  const t = useTranslations('access');
  const { roleId } = Route.useParams();
  const permissions = usePermissions();
  const { data: roles } = useSuspenseQuery(userQueries.roles());
  const { data: catalog } = useSuspenseQuery(accessQueries.catalog());
  const role = roles.find((r) => String(r.id) === roleId);
  // Gone: deleted from this very page, or a stale link. The list it
  // belonged to is where to land.
  if (!role) return <Navigate to="/settings/roles" replace />;

  return (
    <>
      <title>{t('roles.meta', { name: role.name })}</title>
      <RoleDocument
        // A different role, or the saved one coming back, starts a fresh draft.
        key={`${role.id}:${JSON.stringify(role)}`}
        role={role}
        catalog={catalog.resources}
        canEdit={permissions.can('roles', 'update')}
        canDelete={permissions.can('roles', 'delete')}
      />
    </>
  );
}

function RoleLoading() {
  const t = useTranslations('access');
  return (
    <PageLoading>
      <title>{t('meta.title')}</title>
      <SettingsHeader title={<Skeleton className="h-6 w-40" />} />
      <Skeleton className="h-64 w-full rounded-2xl" />
    </PageLoading>
  );
}
