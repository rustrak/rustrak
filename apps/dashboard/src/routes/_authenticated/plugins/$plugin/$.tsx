import { noop } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { pluginQueries } from '@/features/plugin/api/queries';
import {
  pageAt,
  pluginSearch,
  viewPage,
} from '@/features/plugin/model/plugins';
import { PluginPageSkeleton } from '@/features/plugin/ui/components/plugin-page';
import { savedViewQueries } from '@/features/saved-view/api/queries';
import { HostedPluginPage } from '@/routes/_authenticated/-components/hosted-plugin-page';
import { PageLoading } from '@/shared/ui/components/page-loading';

/** A plugin's page outside any project; its folders under src/ui/routes/instance/ name it. */
export const Route = createFileRoute('/_authenticated/plugins/$plugin/$')({
  validateSearch: pluginSearch,
  loader: ({ context: { queryClient }, params }) => {
    void queryClient.query(pluginQueries.list()).catch(noop);
    void queryClient
      .query(
        savedViewQueries.list(
          undefined,
          viewPage(params.plugin, pageAt(params._splat)),
        ),
      )
      .catch(noop);
  },
  pendingComponent: PluginPageLoading,
  component: InstancePluginPage,
});

function InstancePluginPage() {
  const { plugin, _splat } = Route.useParams();
  return (
    <HostedPluginPage
      key={plugin}
      pluginId={plugin}
      splat={_splat}
      search={Route.useSearch()}
    />
  );
}

function PluginPageLoading() {
  return (
    <PageLoading>
      <PluginPageSkeleton />
    </PageLoading>
  );
}
