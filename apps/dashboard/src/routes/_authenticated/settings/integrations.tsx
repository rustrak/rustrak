import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { alertQueries } from '@/features/alert/api/queries';
import { IntegrationsList } from '@/features/alert/ui/components/integrations-list/integrations-list';
import { translator } from '@/shared/i18n/intl';
import { LoadFailure } from '@/shared/ui/components/load-failure';

export const Route = createFileRoute('/_authenticated/settings/integrations')({
  head: () => {
    const t = translator('settings');
    return {
      meta: [
        { title: t('integrations.meta.title') },
        { name: 'description', content: t('integrations.meta.description') },
      ],
    };
  },
  loader: ({ context: { queryClient } }) =>
    queryClient.ensureQueryData(alertQueries.integrations()),
  component: IntegrationsPage,
});

function IntegrationsPage() {
  const t = useTranslations('settings');
  const { data: integrations } = useSuspenseQuery(alertQueries.integrations());

  if (!integrations.success) {
    return (
      <LoadFailure
        error={integrations.error}
        title={t('integrations.loadFailed')}
        notFoundOnMissing={false}
      />
    );
  }

  return (
    <>
      <div className="mb-6 md:mb-8">
        <h1 className="text-xl md:text-2xl font-extrabold tracking-tight">
          {t('integrations.title')}
        </h1>
        <p className="text-muted-foreground mt-1 max-w-2xl">
          {t('integrations.subtitle')}
        </p>
      </div>

      <IntegrationsList initialIntegrations={integrations.data} />
    </>
  );
}
