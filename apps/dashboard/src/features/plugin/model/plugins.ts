import type { Action, PluginSummary } from '@rustrak/client';
import { pluginHref, pluginPermissionKey } from '@rustrak/plugin/ui';

/** Whether a plugin runs in a project: everywhere, or chosen there. */
export function activeIn(plugin: PluginSummary, projectId: number): boolean {
  return plugin.allProjects || plugin.projects.includes(projectId);
}

/** The projects a plugin was chosen in, with `projectId` turned on or off. */
export function withProject(
  plugin: PluginSummary,
  projectId: number,
  on: boolean,
): number[] {
  const next = new Set(plugin.projects);
  if (on) next.add(projectId);
  else next.delete(projectId);
  return [...next];
}

/**
 * A plugin page's address query as text: the plugin reads its own filters,
 * so the dashboard keeps every value that is plain text and drops the rest.
 */
export function pluginSearch(
  search: Record<string, unknown>,
): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(search)) {
    if (typeof value === 'string' && value !== '') out[key] = value;
    else if (typeof value === 'number' || typeof value === 'boolean')
      out[key] = String(value);
  }
  return out;
}

/** `/funnels/42/` → `funnels/42`: the page's address under the plugin. */
export function pageAt(splat: string | undefined): string {
  return (splat ?? '').replace(/^\/+|\/+$/g, '');
}

/** The page of `scope` whose path matches `at`, with its `:name` parameters. */
export function matchPage<P extends { path: string; scope: string }>(
  pages: P[],
  scope: string,
  at: string,
): { page: P; params: Record<string, string> } | null {
  const segments = at ? at.split('/') : [];
  for (const page of pages) {
    if (page.scope !== scope) continue;
    const pattern = page.path ? page.path.split('/') : [];
    if (pattern.length !== segments.length) continue;
    const params: Record<string, string> = {};
    const matches = pattern.every((part, i) => {
      const segment = segments[i]!;
      if (part.startsWith(':')) {
        params[part.slice(1)] = decodeURIComponent(segment);
        return true;
      }
      return part === segment;
    });
    if (matches) return { page, params };
  }
  return null;
}

/** A plugin page's address: in a project, or outside any. */
export const pageHref = (pluginId: string, path: string, projectId?: number) =>
  pluginHref({ pluginId, projectId }, path);

/** Sidebar pages by `rank`; one with none goes last. */
export const bySidebarRank = (
  a: { sidebar?: { rank?: number | null } | null },
  b: { sidebar?: { rank?: number | null } | null },
) =>
  (a.sidebar?.rank ?? Number.MAX_SAFE_INTEGER) -
  (b.sidebar?.rank ?? Number.MAX_SAFE_INTEGER);

/** A plugin page's key among saved views: at most the server's 64 characters. */
export function viewPage(pluginId: string, at: string): string {
  return `plugin:${pluginId}/${at}`.slice(0, 64);
}

/**
 * The section a plugin page belongs to: the sidebar page whose path it is
 * under, the deepest one. `visitors/:id` is under `visitors`, and every page
 * is under the plugin's first (`''`).
 */
export function sectionOf<P extends { path: string; sidebar?: unknown }>(
  pages: readonly P[],
  path: string,
): P | undefined {
  return pages
    .filter(
      (p) =>
        p.sidebar &&
        (p.path === '' || path === p.path || path.startsWith(`${p.path}/`)),
    )
    .sort((a, b) => b.path.length - a.path.length)[0];
}

/** A plugin's page in the settings: what it is, and its settings. */
export const pluginSettingsHref = (pluginId: string) =>
  `/settings/plugins/${pluginId}`;

/**
 * A plugin's README in the reader's language: exact (`pt-BR`), then its base
 * (`es` for `es-ES`), then English, which every plugin ships.
 */
export function readmeIn(
  readme: Record<string, string>,
  locale: string,
): string {
  return (
    readme[locale] ?? readme[locale.split('-')[0] ?? ''] ?? readme.en ?? ''
  );
}

/**
 * Whether the reader may open a plugin page: any that requires nothing, or
 * one whose `resource:action` they hold on the plugin.
 */
export function pageAllowed(
  page: { requires?: string | null },
  pluginId: string,
  can: (key: string, action: Action, projectId?: number) => boolean,
  projectId?: number,
): boolean {
  if (!page.requires) return true;
  const [resource, action] = page.requires.split(':');
  return can(
    pluginPermissionKey(pluginId, resource ?? ''),
    action as Action,
    projectId,
  );
}
