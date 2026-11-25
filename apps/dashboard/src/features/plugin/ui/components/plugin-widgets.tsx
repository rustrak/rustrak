import type { WidgetProps, Zone } from '@rustrak/plugin/ui';
import { usePluginWidgets } from '../hooks/use-plugin-widgets';
import { PluginBoundary } from './plugin-boundary';
import { PluginScope } from './plugin-scope';

/**
 * Where plugins draw into a dashboard page: every widget for `zone` of the
 * plugins active in the project, each inside its own boundary.
 */
export function PluginWidgets<Z extends Zone>({
  zone,
  projectId,
  pluginId,
  props,
}: {
  zone: Z;
  projectId: number;
  /** Only this plugin's: its own setup, in its settings. */
  pluginId?: string;
  props: WidgetProps<Z>;
}) {
  const drawn = usePluginWidgets(zone, projectId, pluginId);
  if (drawn.length === 0) return null;
  return (
    <div className="flex flex-col gap-3" data-zone={zone}>
      {drawn.map(({ key, id, messages, Widget }) => (
        <PluginScope
          key={key}
          pluginId={id}
          projectId={projectId}
          messages={messages}
        >
          <PluginBoundary pluginId={id}>
            <Widget {...props} />
          </PluginBoundary>
        </PluginScope>
      ))}
    </div>
  );
}
