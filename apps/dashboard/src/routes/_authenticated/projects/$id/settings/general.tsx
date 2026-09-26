import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { GeneralSettingsForm } from '@/features/project/ui/components/general-settings-form/general-settings-form';
import { translator } from '@/shared/i18n/intl';
import { LoadFailure } from '@/shared/ui/components/load-failure';

export const Route = createFileRoute(
  '/_authenticated/projects/$id/settings/general',
)({
  head: () => ({
    meta: [{ title: translator('settings')('general.meta.title') }],
  }),
  loader: ({ params: { id }, context: { queryClient } }) =>
    queryClient.ensureQueryData(projectQueries.detail(id)),
  component: GeneralSettingsPage,
});

function GeneralSettingsPage() {
  const t = useTranslations('settings');
  const { id } = Route.useParams();
  const { data: project } = useSuspenseQuery(projectQueries.detail(id));

  if (!project.success) {
    return <LoadFailure error={project.error} title={t('loadProjectFailed')} />;
  }

  return (
    <>
      <div className="mb-6 md:mb-8">
        <h1 className="text-xl md:text-2xl font-extrabold tracking-tight">
          {t('general.title')}
        </h1>
        <p className="text-muted-foreground mt-1">{t('general.subtitle')}</p>
      </div>

      <GeneralSettingsForm project={project.data} />
    </>
  );
}
