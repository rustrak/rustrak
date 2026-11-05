import { Skeleton } from '@rustrak/ui';
import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { ClientKeysSettings } from '@/features/project/ui/components/client-keys-settings';
import { NotAuthorized } from '@/shared/ui/components/not-authorized';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { SettingSection } from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { usePermissions } from '@/shared/ui/hooks/use-permissions';

export const Route = createFileRoute(
  '/_authenticated/projects/$id/settings/client-keys',
)({
  pendingComponent: ClientKeysLoading,
  component: ClientKeysPage,
});

function ClientKeysPage() {
  const t = useTranslations('settings');
  const { id } = Route.useParams();
  // Started by the project layout's loader.
  const { data: project } = useSuspenseQuery(
    projectQueries.detail(Number.parseInt(id, 10)),
  );
  const canRead = usePermissions().can('client-keys', 'read', project.id);

  return (
    <>
      <title>{t('clientKeys.meta.title')}</title>
      <SettingsHeader
        title={t('clientKeys.title')}
        description={t('clientKeys.subtitle')}
      />

      {canRead ? <ClientKeysSettings project={project} /> : <NotAuthorized />}
    </>
  );
}

function ClientKeysLoading() {
  const t = useTranslations('settings');
  const tProjects = useTranslations('projects');
  return (
    <PageLoading>
      <title>{t('clientKeys.meta.title')}</title>
      <SettingsHeader
        title={t('clientKeys.title')}
        description={t('clientKeys.subtitle')}
      />
      <SettingSection
        title={tProjects('clientKeys.dsn')}
        description={tProjects('clientKeys.dsnDescription')}
      >
        <div className="px-5 py-3">
          <Skeleton className="h-8 w-full rounded-lg" />
        </div>
      </SettingSection>
    </PageLoading>
  );
}
