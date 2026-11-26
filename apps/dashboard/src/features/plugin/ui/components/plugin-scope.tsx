import type { Action } from '@rustrak/client';
import {
  type HostComponents,
  type Messages,
  PluginContext,
  type TrailCrumb,
} from '@rustrak/plugin/ui';
import { useQuery } from '@tanstack/react-query';
import { type ReactNode, useMemo } from 'react';
import { usePermissions } from '@/shared/ui/hooks/use-permissions';
import { useRouter } from '@/shared/ui/hooks/use-router';
import { pluginQueries } from '../../api/queries';
import { permissionScope } from '../../model/plugins';

/** What a plugin's page or widget reads from `@rustrak/plugin/ui`. */
export function PluginScope({
  pluginId,
  projectId,
  messages,
  search,
  setSearch,
  setTrail,
  components,
  children,
}: {
  pluginId: string;
  projectId?: number;
  messages: Record<string, Messages>;
  /** A page's address query and its setter; a widget has none. */
  search?: Record<string, string>;
  setSearch?: (next: Record<string, string>) => void;
  setTrail?: (crumbs: TrailCrumb[]) => void;
  components?: HostComponents;
  children: ReactNode;
}) {
  const { push } = useRouter();
  const { canPlugin } = usePermissions();
  const plugin = useQuery(pluginQueries.list()).data?.find(
    (p) => p.id === pluginId,
  );
  const value = useMemo(
    () => ({
      pluginId,
      projectId,
      messages,
      navigate: push,
      can: (key: string, action: Action, at?: number) =>
        canPlugin(key, action, at, plugin && permissionScope(plugin, key)),
      search,
      setSearch,
      setTrail,
      components,
    }),
    [
      pluginId,
      projectId,
      messages,
      push,
      canPlugin,
      plugin,
      search,
      setSearch,
      setTrail,
      components,
    ],
  );
  return (
    <PluginContext.Provider value={value}>{children}</PluginContext.Provider>
  );
}
