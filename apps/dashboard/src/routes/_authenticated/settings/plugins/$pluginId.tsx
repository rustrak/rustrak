import {
  SettingSection,
  SettingsHeader,
  SettingsListSkeleton,
} from '@rustrak/ui';
import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, notFound } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { pluginQueries } from '@/features/plugin/api/queries';
import { PluginDetail } from '@/features/plugin/ui/components/plugin-detail';
import { projectQueries } from '@/features/project/api/queries';
import { searchOneOf } from '@/shared/lib/search-params';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';
import { usePermissions } from '@/shared/ui/hooks/use-permissions';

const projectsToPick = projectQueries.picker();

const TABS = ['settings'] as const;

export const Route = createFileRoute(
  '/_authenticated/settings/plugins/$pluginId',
)({
  // Its README by default; `?tab=settings` for where it runs and its setup.
  validateSearch: (search: Record<string, unknown>) => ({
    tab: searchOneOf(search.tab, TABS),
  }),
  loaderDeps: ({ search }) => ({ tab: search.tab }),
  loader: ({ context: { queryClient }, params, deps }) => {
    void queryClient.query(pluginQueries.list()).catch(noop);
    void queryClient.query(projectsToPick).catch(noop);
    if (deps.tab === 'settings') {
      void queryClient
        .query(pluginQueries.settings(params.pluginId))
        .catch(noop);
    }
  },
  pendingComponent: PluginLoading,
  component: PluginSettingsPage,
});

/**
 * One plugin, in two tabs: its README, and its configuration (where it runs,
 * its settings, its access, and removing it).
 */
function PluginSettingsPage() {
  const { pluginId } = Route.useParams();
  const { tab } = Route.useSearch();
  const navigate = Route.useNavigate();
  const { data: plugins } = useSuspenseQuery(pluginQueries.list());
  const { data: projects } = useQuery(projectsToPick);
  const permissions = usePermissions();
  const plugin = plugins.find((p) => p.id === pluginId);
  usePageTrail(plugin ? [{ label: plugin.name }] : []);
  if (!plugin) throw notFound();
  return (
    <>
      <title>{plugin.name}</title>
      <PluginDetail
        plugin={plugin}
        // Only the picker's first 100 projects are listed; paging or searching
        // them is a separate change.
        projects={projects?.items ?? []}
        canManage={permissions.isOwner}
        tab={tab ?? 'about'}
        onTab={(next) =>
          navigate({
            search: (s) => ({ ...s, tab: next === 'about' ? undefined : next }),
          })
        }
      />
    </>
  );
}

function PluginLoading() {
  const t = useTranslations('settings');
  return (
    <PageLoading>
      <SettingsHeader title={t('plugins.title')} />
      <SettingSection title={t('plugins.detail.projects')}>
        <SettingsListSkeleton rows={2} />
      </SettingSection>
    </PageLoading>
  );
}
