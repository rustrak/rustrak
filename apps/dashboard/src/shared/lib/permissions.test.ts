import type { MyAccess } from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import { permissionsFrom } from './permissions';

const none: MyAccess = {
  is_owner: false,
  instance: [],
  all_projects: [],
  projects: [],
};

describe('permissionsFrom', () => {
  it('grants nothing by default', () => {
    const p = permissionsFrom(none);
    expect(p.isOwner).toBe(false);
    expect(p.can('storage', 'read')).toBe(false);
    expect(p.can('issues', 'read', 1)).toBe(false);
  });

  it('lets an owner do everything', () => {
    const p = permissionsFrom({ ...none, is_owner: true });
    expect(p.can('users', 'delete')).toBe(true);
    expect(p.can('issues', 'delete', 7)).toBe(true);
  });

  it('creates a project only with a way to reach it afterwards', () => {
    const create = { resource: 'projects', action: 'create' } as const;
    const read = { resource: 'issues', action: 'read' } as const;
    expect(
      permissionsFrom({ ...none, instance: [create] }).canCreateProject,
    ).toBe(false);
    expect(
      permissionsFrom({ ...none, all_projects: [read] }).canCreateProject,
    ).toBe(false);
    expect(
      permissionsFrom({
        ...none,
        instance: [create],
        all_projects: [read],
      }).canCreateProject,
    ).toBe(true);
    expect(
      permissionsFrom({
        ...none,
        instance: [create],
        projects: [{ project_id: 1, permissions: [read] }],
      }).canCreateProject,
    ).toBe(true);
    expect(permissionsFrom({ ...none, is_owner: true }).canCreateProject).toBe(
      true,
    );
  });

  it('reads workspace permissions without naming a project', () => {
    const p = permissionsFrom({
      ...none,
      instance: [{ resource: 'storage', action: 'read' }],
    });
    expect(p.can('storage', 'read')).toBe(true);
    expect(p.can('storage', 'update')).toBe(false);
    expect(p.can('project', 'read', 1)).toBe(false);
  });

  it('reaches every project from an all-projects permission', () => {
    const p = permissionsFrom({
      ...none,
      all_projects: [{ resource: 'issues', action: 'update' }],
    });
    expect(p.can('issues', 'update', 3)).toBe(true);
    expect(p.can('issues', 'update', 9)).toBe(true);
    expect(p.can('issues', 'update')).toBe(false);
    expect(p.can('issues', 'delete', 3)).toBe(false);
    expect(p.can('project', 'read', 9)).toBe(true);
  });

  it('keeps a project grant on its project', () => {
    const p = permissionsFrom({
      ...none,
      projects: [
        { project_id: 2, permissions: [{ resource: 'logs', action: 'read' }] },
      ],
    });
    expect(p.can('logs', 'read', 2)).toBe(true);
    expect(p.can('logs', 'read', 3)).toBe(false);
  });

  it('implies seeing a project from any permission on it', () => {
    const p = permissionsFrom({
      ...none,
      projects: [
        { project_id: 2, permissions: [{ resource: 'logs', action: 'read' }] },
      ],
    });
    expect(p.can('project', 'read', 2)).toBe(true);
    expect(p.can('project', 'read', 3)).toBe(false);
    expect(p.can('project', 'update', 2)).toBe(false);
  });

  it('answers "anywhere" when no project is named for a project resource', () => {
    const p = permissionsFrom({
      ...none,
      all_projects: [{ resource: 'logs', action: 'read' }],
      projects: [
        {
          project_id: 2,
          permissions: [{ resource: 'project-members', action: 'update' }],
        },
      ],
    });
    expect(p.canAnywhere('logs', 'read')).toBe(true);
    expect(p.canAnywhere('project-members', 'update')).toBe(true);
    expect(p.canAnywhere('project-members', 'read')).toBe(false);
  });
});

describe('canEverywhere', () => {
  const logs = { resource: 'logs', action: 'read' } as const;
  const storage = { resource: 'storage', action: 'read' } as const;
  const empty = {
    is_owner: false,
    instance: [],
    all_projects: [],
    projects: [],
  };

  it('needs the grant on every project: one project is not enough', () => {
    const one = permissionsFrom({
      ...empty,
      projects: [{ project_id: 1, permissions: [logs] }],
    });
    expect(one.canAnywhere('logs', 'read')).toBe(true);
    expect(one.canEverywhere('logs', 'read')).toBe(false);
  });

  it('counts all-project and workspace grants, and owners', () => {
    const wide = permissionsFrom({
      ...empty,
      all_projects: [logs],
      instance: [storage],
    });
    expect(wide.canEverywhere('logs', 'read')).toBe(true);
    expect(wide.canEverywhere('storage', 'read')).toBe(true);
    expect(
      permissionsFrom({ ...empty, is_owner: true }).canEverywhere(
        'logs',
        'read',
      ),
    ).toBe(true);
  });
});
