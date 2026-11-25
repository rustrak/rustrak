import { pluginKeys } from '@rustrak/plugin/ui';
import { mutationOptions } from '@tanstack/react-query';
import { accessQueries } from '@/shared/api/permissions';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';
import { pluginQueries } from './queries';

export const pluginMutations = {
  /** Installs a `.rtp` package, or replaces the installed version of it. */
  install: () =>
    mutationOptions({
      mutationFn: (file: Blob) =>
        orThrow(api().plugins.install({ body: file })),
      // Its permissions join the catalog Roles is drawn from.
      meta: { invalidates: [pluginQueries.all(), accessQueries.root()] },
    }),
  /** Installs a published plugin from the signed registry: verified. */
  installFromRegistry: () =>
    mutationOptions({
      mutationFn: (pluginId: string) =>
        orThrow(
          api().plugins.installFromRegistry({ path: { plugin: pluginId } }),
        ),
      meta: { invalidates: [pluginQueries.all(), accessQueries.root()] },
    }),
  /** Where a plugin runs: every project, or the ones chosen. */
  configure: (pluginId: string) =>
    mutationOptions({
      mutationFn: (body: { allProjects: boolean; projects?: number[] }) =>
        orThrow(api().plugins.configure({ path: { plugin: pluginId }, body })),
      meta: { invalidates: [pluginQueries.all()] },
    }),
  /** Writes a plugin's settings, for the whole instance or one project. */
  saveSettings: (pluginId: string, projectId?: number) =>
    mutationOptions({
      mutationFn: (body: Record<string, unknown>) =>
        orThrow(
          projectId === undefined
            ? api().plugins.setSettings({ path: { plugin: pluginId }, body })
            : api().plugins.setProjectSettings({
                path: { plugin: pluginId, project_id: projectId },
                body,
              }),
        ),
      meta: { invalidates: [pluginKeys.all(pluginId)] },
    }),
  /** Removes a plugin; its data too with `purge`. */
  uninstall: (pluginId: string) =>
    mutationOptions({
      mutationFn: (purge: boolean) =>
        orThrow(
          api().plugins.uninstall({
            path: { plugin: pluginId },
            query: { purge },
          }),
        ),
      meta: { invalidates: [pluginQueries.all(), accessQueries.root()] },
    }),
};
