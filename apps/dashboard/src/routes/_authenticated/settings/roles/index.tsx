import { SecurityIcon } from '@rustrak/ui';
import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { userQueries } from '@/features/user/api/queries';
import { RolesList } from '@/features/user/ui/components/roles-list';
import { startIfAllowed } from '@/shared/api/permissions';
import { PageLoading } from '@/shared/ui/components/page-loading';
import {
  SettingSection,
  SettingsEmpty,
  SettingsListSkeleton,
} from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { usePermissions } from '@/shared/ui/hooks/use-permissions';

export const Route = createFileRoute('/_authenticated/settings/roles/')({
  loader: ({ context: { queryClient } }) => {
    startIfAllowed(
      queryClient,
      (p) => p.can('roles', 'read'),
      () => void queryClient.query(userQueries.roles()).catch(noop),
    );
  },
  pendingComponent: RolesLoading,
  component: RolesPage,
});

function RolesPage() {
  const t = useTranslations('access');
  const permissions = usePermissions();

  return (
    <>
      <title>{t('meta.title')}</title>
      <SettingsHeader title={t('title')} description={t('subtitle')} />
      {permissions.can('roles', 'read') ? (
        <Roles canCreate={permissions.can('roles', 'create')} />
      ) : (
        <div className="overflow-hidden rounded-2xl border border-border-subtle bg-surface">
          <SettingsEmpty
            icon={<SecurityIcon size="2xl" />}
            title={t('notAuthorized')}
            description={t('notAuthorizedDescription')}
          />
        </div>
      )}
    </>
  );
}

/** Separate so the roles are only read by who may read them. */
function Roles({ canCreate }: { canCreate: boolean }) {
  const { data: roles } = useSuspenseQuery(userQueries.roles());
  return <RolesList roles={roles} canCreate={canCreate} />;
}

function RolesLoading() {
  const t = useTranslations('access');
  return (
    <PageLoading>
      <title>{t('meta.title')}</title>
      <SettingsHeader title={t('title')} description={t('subtitle')} />
      <SettingSection title={t('roles.title')}>
        <SettingsListSkeleton />
      </SettingSection>
    </PageLoading>
  );
}
