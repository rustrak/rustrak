import { Skeleton } from '@rustrak/ui';
import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { GeneralSettingsForm } from '@/features/project/ui/components/general-settings-form/general-settings-form';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { SettingRow, SettingSection } from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';

export const Route = createFileRoute(
  '/_authenticated/projects/$id/settings/general',
)({
  pendingComponent: GeneralSettingsLoading,
  component: GeneralSettingsPage,
});

function GeneralSettingsPage() {
  const t = useTranslations('settings');
  const { id } = Route.useParams();
  // Started by the project layout's loader.
  const { data: project } = useSuspenseQuery(
    projectQueries.detail(Number.parseInt(id, 10)),
  );

  return (
    <>
      <title>{t('general.meta.title')}</title>
      <SettingsHeader
        title={t('general.title')}
        description={t('general.subtitle')}
      />

      <GeneralSettingsForm project={project} />
    </>
  );
}

function GeneralSettingsLoading() {
  const t = useTranslations('settings');
  const tProjects = useTranslations('projects');
  const rows: [string, string][] = [
    [tProjects('nameTitle'), tProjects('nameDescription')],
    [tProjects('fields.slug'), tProjects('slugDescription')],
    [tProjects('fields.platform'), tProjects('platformDescription')],
  ];
  return (
    <PageLoading>
      <title>{t('general.meta.title')}</title>
      <SettingsHeader
        title={t('general.title')}
        description={t('general.subtitle')}
      />
      <SettingSection title={tProjects('projectDetails')}>
        {rows.map(([title, description]) => (
          <SettingRow key={title} title={title} description={description}>
            <Skeleton className="h-9 w-full rounded-lg" />
          </SettingRow>
        ))}
      </SettingSection>
    </PageLoading>
  );
}
