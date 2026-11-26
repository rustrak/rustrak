import type {
  Action,
  Catalog,
  CatalogResource,
  Permission,
  PluginGrant,
  Resource,
} from '@rustrak/client';

/**
 * A row of the permission editor. A plugin's permission is one too, keyed
 * `<plugin>:<resource>` and grouped under its plugin, with its own texts by
 * language where Rustrak's come from the dashboard's messages.
 */
export type EditorResource = CatalogResource & {
  label?: Record<string, string>;
  help?: Record<string, string>;
  /** The plugin's name, for its tab. */
  moduleName?: string;
};

/** Rustrak's resources, then each plugin's, as one list of rows. */
export function editorCatalog(catalog: Catalog): EditorResource[] {
  return [
    ...catalog.resources,
    ...catalog.plugins.map((p) => ({
      key: p.key as Resource,
      module: `plugin:${p.plugin}`,
      scope: p.scope,
      actions: p.actions,
      label: p.label,
      help: p.description,
      moduleName: p.plugin_name,
    })),
  ];
}

/** A plugin's permission: its key names the plugin. */
export const isPluginKey = (resource: string) => resource.includes(':');

/** The editor's one list, as the API keeps it: Rustrak's, and plugins'. */
export function splitPermissions(all: Permission[]): {
  permissions: Permission[];
  plugin_permissions: PluginGrant[];
} {
  return {
    permissions: all.filter((p) => !isPluginKey(p.resource)),
    plugin_permissions: all.filter((p) => isPluginKey(p.resource)),
  };
}

/** The API's two lists as the editor's one. */
export function joinPermissions(
  permissions: Permission[],
  plugins: PluginGrant[] = [],
): Permission[] {
  return [...permissions, ...(plugins as Permission[])];
}

/** The columns of a permission matrix. */
export const ACTIONS: Action[] = ['read', 'create', 'update', 'delete'];

/** Instance-wide modules first, then a project's, as the server lists them. */
const MODULE_ORDER = [
  'workspace',
  'settings',
  'issues',
  'monitoring',
  'releases',
  'alerts',
  'project',
];

export interface Module {
  module: string;
  resources: EditorResource[];
}

export function modulesOf(catalog: EditorResource[]): Module[] {
  const rank = (m: string) => {
    const i = MODULE_ORDER.indexOf(m);
    return i === -1 ? MODULE_ORDER.length : i;
  };
  // Plugins after every module of Rustrak's own.
  const modules = [...new Set(catalog.map((r) => r.module))].sort(
    (a, b) => rank(a) - rank(b),
  );
  return modules.map((module) => ({
    module,
    resources: catalog.filter((r) => r.module === module),
  }));
}

/**
 * The catalog narrowed to what the reader holds: they cannot hand out the rest,
 * so an editor should not offer it.
 */
export function grantable<R extends CatalogResource>(
  catalog: R[],
  reader: { canAnywhere: (resource: Resource, action: Action) => boolean },
): R[] {
  return catalog
    .map((resource) => ({
      ...resource,
      actions: resource.actions.filter((action) =>
        reader.canAnywhere(resource.key as Resource, action),
      ),
    }))
    .filter((resource) => resource.actions.length > 0);
}

export const hasPermission = (
  permissions: Permission[],
  resource: Resource | string,
  action: Action,
) => permissions.some((p) => p.resource === resource && p.action === action);

export function toggle(
  permissions: Permission[],
  resource: Resource | string,
  action: Action,
): Permission[] {
  return hasPermission(permissions, resource, action)
    ? permissions.filter(
        (p) => !(p.resource === resource && p.action === action),
      )
    : [...permissions, { resource: resource as Resource, action }];
}

export type RowState = 'none' | 'some' | 'all';

export function rowState(
  permissions: Permission[],
  resource: CatalogResource,
): RowState {
  const held = resource.actions.filter((a) =>
    hasPermission(permissions, resource.key, a),
  ).length;
  return held === 0
    ? 'none'
    : held === resource.actions.length
      ? 'all'
      : 'some';
}

/** All of a row's actions on, or (when it already has all) all off. */
export function toggleRow(
  permissions: Permission[],
  resource: CatalogResource,
): Permission[] {
  // Only the actions this row shows: a narrowed editor keeps what it hides.
  const shown = new Set(resource.actions);
  const rest = permissions.filter(
    (p) => !(p.resource === resource.key && shown.has(p.action)),
  );
  return rowState(permissions, resource) === 'all'
    ? rest
    : [
        ...rest,
        ...resource.actions.map((action) => ({
          resource: resource.key as Resource,
          action,
        })),
      ];
}
