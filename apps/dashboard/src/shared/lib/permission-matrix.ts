import type {
  Action,
  CatalogResource,
  Permission,
  Resource,
} from '@rustrak/client';

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
  resources: CatalogResource[];
}

export function modulesOf(catalog: CatalogResource[]): Module[] {
  const rank = (m: string) => {
    const i = MODULE_ORDER.indexOf(m);
    return i === -1 ? MODULE_ORDER.length : i;
  };
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
export function grantable(
  catalog: CatalogResource[],
  reader: { canAnywhere: (resource: Resource, action: Action) => boolean },
): CatalogResource[] {
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
  const rest = permissions.filter((p) => p.resource !== resource.key);
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
