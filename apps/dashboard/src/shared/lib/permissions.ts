import type { Action, MyAccess, Resource, Scope } from '@rustrak/client';
import { hasPermission as has, isPluginKey } from './permission-matrix';

/**
 * What the reader may do, from `GET /api/access/me`. Mirrors `Access::can`
 * in `apps/server/src/auth/rbac.rs`; the server enforces, this only decides
 * what to show.
 */
export interface Permissions {
  isOwner: boolean;
  /** On the workspace, or on `projectId` for a project resource. */
  can: (resource: Resource, action: Action, projectId?: number) => boolean;
  /** On the workspace or on at least one project. */
  canAnywhere: (resource: Resource, action: Action) => boolean;
  /** On the workspace or on every project: what a role of the list can hold. */
  canEverywhere: (resource: Resource, action: Action) => boolean;
  /**
   * `projects:create` and a project permission: the creator's own roles are
   * what reach a new project, so with none the server refuses.
   */
  canCreateProject: boolean;
  /**
   * A plugin's permission, `<plugin>:<resource>`, as the server checks it:
   * one declared with `instance` scope holds wherever a role giving it is
   * held, so it ignores `projectId`.
   */
  canPlugin: (
    key: string,
    action: Action,
    projectId?: number,
    scope?: Scope,
  ) => boolean;
}

export const NO_PERMISSIONS: MyAccess = {
  is_owner: false,
  instance: [],
  all_projects: [],
  projects: [],
  plugins: { everywhere: [], projects: [] },
};

/** Writing implies reading, as on the server. */
const holds = (
  grants: { resource: string; action: Action }[],
  key: string,
  action: Action,
) =>
  grants.some(
    (g) => g.resource === key && (g.action === action || action === 'read'),
  );

export function permissionsFrom(me: MyAccess): Permissions {
  const local = (projectId: number) =>
    me.projects.find((p) => p.project_id === projectId)?.permissions ?? [];
  const canPlugin: Permissions['canPlugin'] = (key, action, projectId, scope) =>
    me.is_owner ||
    holds(me.plugins.everywhere, key, action) ||
    (projectId === undefined || scope === 'instance'
      ? me.plugins.projects.some((p) => holds(p.permissions, key, action))
      : holds(
          me.plugins.projects.find((p) => p.project_id === projectId)
            ?.permissions ?? [],
          key,
          action,
        ));

  return {
    canCreateProject:
      me.is_owner ||
      (has(me.instance, 'projects', 'create') &&
        (me.all_projects.length > 0 || me.projects.length > 0)),
    isOwner: me.is_owner,
    can: (resource, action, projectId) => {
      if (me.is_owner || has(me.instance, resource, action)) return true;
      if (projectId === undefined) return false;
      // Any permission on a project implies seeing it.
      if (resource === 'project' && action === 'read') {
        return me.all_projects.length > 0 || local(projectId).length > 0;
      }
      return (
        has(me.all_projects, resource, action) ||
        has(local(projectId), resource, action)
      );
    },
    canAnywhere: (resource, action) =>
      isPluginKey(resource)
        ? canPlugin(resource, action)
        : me.is_owner ||
          has(me.instance, resource, action) ||
          has(me.all_projects, resource, action) ||
          me.projects.some((p) => has(p.permissions, resource, action)),
    canPlugin,
    canEverywhere: (resource, action) =>
      me.is_owner ||
      (isPluginKey(resource)
        ? holds(me.plugins.everywhere, resource, action)
        : has(me.instance, resource, action) ||
          has(me.all_projects, resource, action)),
  };
}
