import type { ProviderType } from '@rustrak/client';
import { Skeleton } from '@rustrak/ui';
import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, notFound } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { alertQueries } from '@/features/alert/api/queries';
import { alertProviders } from '@/features/alert/model/providers';
import {
  type EditParam,
  ProviderSettings,
} from '@/features/alert/ui/components/provider-settings';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { SettingsListSkeleton } from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';
import { integrationProjectId } from '../-components/integrations-back-link';

export const Route = createFileRoute(
  '/_authenticated/settings/integrations/$provider',
)({
  validateSearch: (search: Record<string, unknown>): { edit?: EditParam } => ({
    edit: search.edit === 'new' ? 'new' : integrationProjectId(search.edit),
  }),
  loader: ({ context: { queryClient } }) => {
    void queryClient.query(alertQueries.integrations()).catch(noop);
  },
  pendingComponent: ProviderLoading,
  component: ProviderPage,
});

/** The provider the address names, if Rustrak has one by that name. */
function useProvider() {
  const { provider } = Route.useParams();
  return alertProviders.find((p) => p.type === provider);
}

/** One alert provider: its integrations, and the form to add or edit one. */
function ProviderPage() {
  const tAlerts = useTranslations('alerts');
  const provider = useProvider();
  const { data: integrations } = useSuspenseQuery(alertQueries.integrations());
  usePageTrail([{ label: provider?.name ?? '' }]);
  const { edit } = Route.useSearch();
  const navigate = Route.useNavigate();
  if (!provider) throw notFound();
  const description = tAlerts(
    provider.descriptionKey as 'providers.slack.description',
  );

  return (
    <>
      <title>{`${provider.name} | Rustrak`}</title>
      <ProviderSettings
        provider={provider.type as ProviderType}
        name={provider.name}
        description={description}
        integrations={integrations.filter(
          (i) => i.provider_type === provider.type,
        )}
        edit={edit}
        onEdit={(next) => navigate({ search: { edit: next } })}
      />
    </>
  );
}

function ProviderLoading() {
  const tAlerts = useTranslations('alerts');
  // An unknown provider is the page's to refuse; here it is only a blank header.
  const provider = useProvider();
  return (
    <PageLoading>
      {provider ? (
        <>
          <title>{`${provider.name} | Rustrak`}</title>
          <SettingsHeader
            title={provider.name}
            description={tAlerts(
              provider.descriptionKey as 'providers.slack.description',
            )}
          />
        </>
      ) : null}
      <Skeleton className="mb-3 h-5 w-40" />
      <div className="overflow-hidden rounded-2xl border border-border-subtle bg-surface">
        <SettingsListSkeleton />
      </div>
    </PageLoading>
  );
}
