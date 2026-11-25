import { noop } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { pluginQueries } from '@/features/plugin/api/queries';
import { pluginSearch } from '@/features/plugin/model/plugins';
import { HostedPluginPage } from '@/routes/_authenticated/-components/hosted-plugin-page';

/** A plugin's page inside a project; its folders under src/ui/routes/project/ name it. */
export const Route = createFileRoute(
  '/_authenticated/projects/$id/plugins/$plugin/$',
)({
  validateSearch: pluginSearch,
  loader: ({ context: { queryClient } }) => {
    void queryClient.query(pluginQueries.list()).catch(noop);
  },
  component: ProjectPluginPage,
});

function ProjectPluginPage() {
  const { id, plugin, _splat } = Route.useParams();
  return (
    <HostedPluginPage
      pluginId={plugin}
      splat={_splat}
      projectId={id}
      search={Route.useSearch()}
    />
  );
}
