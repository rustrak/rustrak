import { pluginTranslator } from '@rustrak/plugin/ui';
import { useMemo } from 'react';
import { useLocale } from 'use-intl';
import { usePermissionsLater } from '@/shared/ui/hooks/use-permissions';
import {
  activeIn,
  bySidebarRank,
  pageAllowed,
  pageHref,
} from '../../model/plugins';
import { useLoadedPlugins } from './use-loaded-plugins';

interface PluginNavEntry {
  key: string;
  label: string;
  /** A `@rustrak/ui` icon name, as the plugin declared it. */
  icon?: string | null;
  href: string;
}

/** One plugin's place in the sidebar: its name over its pages. */
export interface PluginNavGroup {
  id: string;
  name: string;
  entries: PluginNavEntry[];
}

/**
 * The plugin pages that asked for a place in the sidebar, grouped by plugin:
 * a project's when `projectId` is given (plugins active there only), the
 * workspace's when not. Each group's pages by `rank`; groups by name.
 */
export function usePluginNav(projectId?: number): PluginNavGroup[] {
  const { loaded } = useLoadedPlugins();
  const locale = useLocale();
  const { canPlugin } = usePermissionsLater();
  return useMemo(() => {
    const scope = projectId === undefined ? 'instance' : 'project';
    return loaded
      .filter(
        ({ plugin }) => projectId === undefined || activeIn(plugin, projectId),
      )
      .map(({ plugin, definition }) => {
        const t = pluginTranslator(definition.messages, locale);
        const entries = (plugin.ui?.pages ?? [])
          .flatMap(({ sidebar, ...page }) =>
            sidebar &&
            page.scope === scope &&
            pageAllowed(page, plugin, canPlugin, projectId)
              ? [{ ...page, sidebar }]
              : [],
          )
          .sort(bySidebarRank)
          .map((page) => ({
            key: `${plugin.id}/${page.path}`,
            label: t(page.sidebar.label),
            icon: page.sidebar.icon,
            href: pageHref(plugin.id, page.path, projectId),
          }));
        return { id: plugin.id, name: plugin.name, entries };
      })
      .filter((group) => group.entries.length > 0)
      .sort((a, b) => a.name.localeCompare(b.name));
  }, [loaded, locale, projectId, canPlugin]);
}

/**
 * The entry a page belongs to: the deepest one its address is under, so a
 * plugin's detail page keeps its section lit.
 */
export function activeEntry(
  entries: readonly PluginNavEntry[],
  pathname: string,
): string | undefined {
  return entries
    .filter((e) => pathname === e.href || pathname.startsWith(`${e.href}/`))
    .sort((a, b) => b.href.length - a.href.length)[0]?.key;
}
