import type { Action, MyAccess, Resource } from '@rustrak/client';
import { hasPermission as has } from './permission-matrix';

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
}

export const NO_PERMISSIONS: MyAccess = {
  is_owner: false,
  instance: [],
  all_projects: [],
  projects: [],
};

export function permissionsFrom(me: MyAccess): Permissions {
  const local = (projectId: number) =>
    me.projects.find((p) => p.project_id === projectId)?.permissions ?? [];

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
      me.is_owner ||
      has(me.instance, resource, action) ||
      has(me.all_projects, resource, action) ||
      me.projects.some((p) => has(p.permissions, resource, action)),
    canEverywhere: (resource, action) =>
      me.is_owner ||
      has(me.instance, resource, action) ||
      has(me.all_projects, resource, action),
  };
}
