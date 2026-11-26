/**
 * Reads for plugins: the instance's under `['plugins']`; one plugin's own
 * data under `pluginKeys`, which its pages share.
 */
import { pluginKeys } from '@rustrak/plugin/ui';
import { queryOptions } from '@tanstack/react-query';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';

export const pluginQueries = {
  all: () => ['plugins'] as const,
  /** The installed plugins and what each adds to the dashboard. */
  list: () =>
    queryOptions({
      queryKey: [...pluginQueries.all(), 'list'],
      queryFn: ({ signal }) => orThrow(api().plugins.list({ signal })),
    }),
  /** What the signed registry publishes, with the version this Rustrak runs. */
  registry: () =>
    queryOptions({
      queryKey: [...pluginQueries.all(), 'registry'],
      queryFn: ({ signal }) => orThrow(api().plugins.registry({ signal })),
    }),
  /**
   * One plugin's settings, for the whole instance or one project: the same
   * entry its pages read with `usePluginSettings()`, so a save shows there.
   */
  settings: (pluginId: string, projectId?: number) =>
    queryOptions({
      queryKey: pluginKeys.settings(pluginId, projectId),
      queryFn: ({ signal }) =>
        orThrow(
          projectId === undefined
            ? api().plugins.settings({ path: { plugin: pluginId }, signal })
            : api().plugins.projectSettings({
                path: { plugin: pluginId, project_id: projectId },
                signal,
              }),
        ),
    }),
};
