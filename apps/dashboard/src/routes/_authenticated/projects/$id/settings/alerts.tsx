import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { alertQueries } from '@/features/alert/api/queries';
import { AlertsSettings } from '@/features/alert/ui/components/alerts-settings';
import { projectQueries } from '@/features/project/api/queries';
import { translator } from '@/shared/i18n/intl';
import { combine, loadAll } from '@/shared/lib/results';
import { LoadFailure } from '@/shared/ui/components/load-failure';

export const Route = createFileRoute(
  '/_authenticated/projects/$id/settings/alerts',
)({
  head: () => ({
    meta: [{ title: translator('settings')('alerts.meta.title') }],
  }),
  // The two lists used to fall back to `[]`, which drew "no alert rules yet"
  // over an outage and invited the admin to recreate rules that already exist.
  loader: ({ params: { id }, context: { queryClient } }) =>
    loadAll([
      queryClient.ensureQueryData(projectQueries.detail(id)),
      queryClient.ensureQueryData(alertQueries.rules(id)),
      queryClient.ensureQueryData(alertQueries.integrations()),
    ]),
  component: AlertsSettingsPage,
});

function AlertsSettingsPage() {
  const t = useTranslations('settings');
  const { id } = Route.useParams();
  const loaded = combine([
    useSuspenseQuery(projectQueries.detail(id)).data,
    useSuspenseQuery(alertQueries.rules(id)).data,
    useSuspenseQuery(alertQueries.integrations()).data,
  ]);

  if (!loaded.success) {
    return <LoadFailure error={loaded.error} title={t('alerts.loadFailed')} />;
  }

  const [project, alertRules, channels] = loaded.data;

  return (
    <AlertsSettings
      project={project}
      alertRules={alertRules}
      channels={channels}
    />
  );
}
