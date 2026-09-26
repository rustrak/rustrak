import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { getProject, getRateLimits } from '@/features/project/api/queries';
import { GeneralSettingsForm } from '@/features/project/ui/components/general-settings-form/general-settings-form';
import { translator } from '@/shared/i18n/intl';
import { LoadFailure } from '@/shared/ui/components/load-failure';

export const Route = createFileRoute(
  '/_authenticated/projects/$id/settings/general',
)({
  head: () => ({
    meta: [{ title: translator('settings')('general.meta.title') }],
  }),
  loader: async ({ params }) => {
    const [project, rateLimits] = await Promise.all([
      getProject(Number.parseInt(params.id, 10)),
      getRateLimits(),
    ]);
    return { project, rateLimits };
  },
  component: GeneralSettingsPage,
});

function GeneralSettingsPage() {
  const t = useTranslations('settings');
  const { project, rateLimits } = Route.useLoaderData();

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

      <GeneralSettingsForm
        project={project.data}
        // A failure here costs only the placeholder, not the page.
        serverLimits={rateLimits.success ? rateLimits.data : null}
      />
    </>
  );
}
