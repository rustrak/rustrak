import type { Zone } from '@rustrak/plugin/ui';
import { activeIn } from '../../model/plugins';
import { useLoadedPlugins } from './use-loaded-plugins';

/** The widgets for `zone` of the plugins active in the project, or of one. */
export function usePluginWidgets(
  zone: Zone,
  projectId: number,
  pluginId?: string,
) {
  const { loaded } = useLoadedPlugins();
  return loaded.flatMap(({ plugin, definition }) =>
    activeIn(plugin, projectId) &&
    (pluginId === undefined || plugin.id === pluginId)
      ? definition.widgets
          .filter((w) => w.zone === zone)
          .map((w, i) => ({
            key: `${plugin.id}/${i}`,
            id: plugin.id,
            messages: definition.messages,
            Widget: w.component,
          }))
      : [],
  );
}
