import { setupHref } from '@rustrak/plugin/ui';
import {
  SettingSection,
  SettingsHeader,
  SettingsListSkeleton,
} from '@rustrak/ui';
import { noop, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, notFound } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { pluginQueries } from '@/features/plugin/api/queries';
import { PluginProjectSetup } from '@/features/plugin/ui/components/plugin-project-setup';
import { projectQueries } from '@/features/project/api/queries';
import { searchId } from '@/shared/lib/search-params';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';
import { usePermissions } from '@/shared/ui/hooks/use-permissions';

/** A plugin in one project: whether it runs, its setup, its settings there. */
export const Route = createFileRoute(
  '/_authenticated/settings/plugins/$pluginId_/projects/$projectId',
)({
  loader: ({ context: { queryClient }, params }) => {
    void queryClient.query(pluginQueries.list()).catch(noop);
    const id = searchId(params.projectId);
    if (id) {
      void queryClient.query(projectQueries.detail(id)).catch(noop);
      void queryClient
        .query(pluginQueries.settings(params.pluginId, id))
        .catch(noop);
    }
  },
  pendingComponent: SetupLoading,
  component: PluginProjectPage,
});

function PluginProjectPage() {
  const { pluginId, projectId: raw } = Route.useParams();
  const projectId = searchId(raw);
  if (projectId === undefined) throw notFound();
  return <ProjectSetup pluginId={pluginId} projectId={projectId} />;
}

function ProjectSetup({
  pluginId,
  projectId,
}: {
  pluginId: string;
  projectId: number;
}) {
  const { data: plugins } = useSuspenseQuery(pluginQueries.list());
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  const permissions = usePermissions();
  const plugin = plugins.find((p) => p.id === pluginId);
  usePageTrail(
    plugin
      ? [
          {
            label: plugin.name,
            href: setupHref({ pluginId: plugin.id }),
          },
          { label: project.name },
        ]
      : [],
  );
  if (!plugin) throw notFound();
  return (
    <>
      <title>{`${plugin.name} · ${project.name}`}</title>
      <PluginProjectSetup
        plugin={plugin}
        project={project}
        canManage={permissions.isOwner}
      />
    </>
  );
}

function SetupLoading() {
  const t = useTranslations('settings');
  return (
    <PageLoading>
      <SettingsHeader title={t('plugins.title')} />
      <SettingSection title={t('plugins.setup.where')}>
        <SettingsListSkeleton rows={1} />
      </SettingSection>
    </PageLoading>
  );
}
