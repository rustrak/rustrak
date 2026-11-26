import { SettingsHeader, SettingsListSkeleton } from '@rustrak/ui';
import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { pluginQueries } from '@/features/plugin/api/queries';
import { catalogSearch } from '@/features/plugin/model/plugins';
import {
  InstallPluginButton,
  PluginCatalog,
} from '@/features/plugin/ui/components/plugin-catalog';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { usePermissions } from '@/shared/ui/hooks/use-permissions';

export const Route = createFileRoute('/_authenticated/settings/plugins/')({
  validateSearch: catalogSearch,
  loader: ({ context: { queryClient } }) => {
    void queryClient.query(pluginQueries.list()).catch(noop);
    // An owner's only; anyone else's 403 is swallowed here and never read.
    void queryClient.query(pluginQueries.registry()).catch(noop);
  },
  pendingComponent: PluginsLoading,
  component: PluginsPage,
});

function PluginsPage() {
  const t = useTranslations('settings');
  const { data: plugins } = useSuspenseQuery(pluginQueries.list());
  const permissions = usePermissions();
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  return (
    <>
      <title>{t('plugins.meta.title')}</title>
      <SettingsHeader
        title={t('plugins.title')}
        description={t('plugins.subtitle')}
        actions={permissions.isOwner ? <InstallPluginButton /> : null}
      />
      <PluginCatalog
        plugins={plugins}
        canInstall={permissions.isOwner}
        query={search.q ?? ''}
        // Only an owner has the filters: anyone else's is always All.
        filter={permissions.isOwner ? (search.filter ?? 'all') : 'all'}
        onSearch={({ q, filter }) =>
          navigate({
            search: catalogSearch({ q, filter }),
            replace: true,
          })
        }
      />
    </>
  );
}

function PluginsLoading() {
  const t = useTranslations('settings');
  return (
    <PageLoading>
      <title>{t('plugins.meta.title')}</title>
      <SettingsHeader
        title={t('plugins.title')}
        description={t('plugins.subtitle')}
      />
      <SettingsListSkeleton rows={2} />
    </PageLoading>
  );
}
