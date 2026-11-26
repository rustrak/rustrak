import type {
  Action,
  PluginSummary,
  RegistryPlugin,
  Scope,
} from '@rustrak/client';
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

/**
 * The page of `scope` whose path matches `at`, with its `:name` parameters:
 * of several, the one with the fewest parameters, so `funnels/new` wins over
 * `funnels/:id`. `at` comes decoded from the router.
 */
export function matchPage<P extends { path: string; scope: string }>(
  pages: P[],
  scope: string,
  at: string,
): { page: P; params: Record<string, string> } | null {
  const segments = at ? at.split('/') : [];
  const params = (page: P) => page.path.split(':').length;
  for (const page of [...pages].sort((a, b) => params(a) - params(b))) {
    if (page.scope !== scope) continue;
    const pattern = page.path ? page.path.split('/') : [];
    if (pattern.length !== segments.length) continue;
    const params: Record<string, string> = {};
    const matches = pattern.every((part, i) => {
      const segment = segments[i]!;
      if (part.startsWith(':')) {
        params[part.slice(1)] = segment;
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

/**
 * A plugin page's key among saved views: at most the server's 64 characters.
 * A longer one keeps its start and ends in a hash of the whole, so two long
 * addresses never share their views; it starts `plugin~`, which no short
 * key does, so none can be mistaken for a short one.
 */
export function viewPage(pluginId: string, at: string): string {
  const key = `plugin:${pluginId}/${at}`;
  if (key.length <= 64) return key;
  let hash = 0x811c9dc5;
  for (let i = 0; i < key.length; i++) {
    hash = Math.imul(hash ^ key.charCodeAt(i), 0x01000193);
  }
  const start = `plugin~${pluginId}/${at}`.slice(0, 55);
  return `${start}~${(hash >>> 0).toString(16).padStart(8, '0')}`;
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

/** The scope a plugin declared one of its permissions with, by its key. */
export function permissionScope(
  plugin: Pick<PluginSummary, 'id' | 'permissions'>,
  key: string,
): Scope | undefined {
  return plugin.permissions.find(
    (p) => pluginPermissionKey(plugin.id, p.resource) === key,
  )?.scope;
}

/**
 * Whether the reader may open a plugin page: any that requires nothing, or
 * one whose `resource:action` they hold on the plugin.
 */
export function pageAllowed(
  page: { requires?: string | null },
  plugin: Pick<PluginSummary, 'id' | 'permissions'>,
  can: (
    key: string,
    action: Action,
    projectId?: number,
    scope?: Scope,
  ) => boolean,
  projectId?: number,
): boolean {
  if (!page.requires) return true;
  const [resource, action] = page.requires.split(':');
  const key = pluginPermissionKey(plugin.id, resource ?? '');
  return can(key, action as Action, projectId, permissionScope(plugin, key));
}

/** One plugin in Settings › Plugins: installed here, published, or both. */
export interface CatalogEntry {
  id: string;
  name: string;
  description: string | null;
  installed?: PluginSummary;
  published?: RegistryPlugin;
}

export type CatalogFilter = 'all' | 'installed' | 'available' | 'updates';

/** Whether the registry publishes a newer version than the installed one. */
export function hasUpdate(entry: CatalogEntry): boolean {
  return (
    !!entry.installed &&
    !!entry.published &&
    entry.published.installed_version !== entry.published.version
  );
}

/**
 * The installed plugins and the published ones as one list by name, kept to
 * `filter` and to those whose name, id or description holds `query`.
 */
export function catalog(
  installed: PluginSummary[],
  registry: RegistryPlugin[] | undefined,
  filter: CatalogFilter,
  query: string,
): CatalogEntry[] {
  const byId = new Map<string, CatalogEntry>();
  for (const p of installed) byId.set(p.id, { ...p, installed: p });
  for (const p of registry ?? []) {
    const entry = byId.get(p.id);
    if (entry) entry.published = p;
    else byId.set(p.id, { ...p, published: p });
  }
  const q = query.trim().toLowerCase();
  return [...byId.values()]
    .filter(
      (e) =>
        filter === 'all' ||
        (filter === 'installed' && !!e.installed) ||
        (filter === 'available' && !e.installed) ||
        (filter === 'updates' && hasUpdate(e)),
    )
    .filter((e) =>
      [e.name, e.id, e.description ?? ''].some((s) =>
        s.toLowerCase().includes(q),
      ),
    )
    .sort((a, b) => a.name.localeCompare(b.name));
}
