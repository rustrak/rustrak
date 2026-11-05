import type { Action, CatalogResource, Resource } from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import {
  reachesProjects,
  rowsFrom,
  sameAccess,
  toSetAccess,
} from './assignments';

const held = (
  role_id: number,
  project_id: number | null,
  is_personal = false,
) => ({
  role_id,
  project_id,
  is_personal,
});
const logs = [{ resource: 'logs' as Resource, action: 'read' as Action }];

describe('access rows', () => {
  it('has no rows for someone with no access', () => {
    expect(rowsFrom([])).toEqual([]);
  });

  it('groups a role by the projects it is held on', () => {
    expect(
      rowsFrom([held(2, null), held(3, 7), held(3, 8), held(9, 7, true)]),
    ).toEqual([
      { role: 2, projects: null },
      { role: 3, projects: [7, 8] },
      { role: 'personal', projects: [7] },
    ]);
  });

  it('lets every project win over a list', () => {
    expect(rowsFrom([held(3, 7), held(3, null)])).toEqual([
      { role: 3, projects: null },
    ]);
  });

  it('expands rows back and leaves out empty ones', () => {
    expect(
      toSetAccess(
        [
          { role: null, projects: null },
          { role: 2, projects: null },
          { role: 3, projects: [7, 8] },
        ],
        logs,
      ),
    ).toEqual({
      assignments: [
        { role_id: 2, project_id: null },
        { role_id: 3, project_id: 7 },
        { role_id: 3, project_id: 8 },
      ],
      personal: null,
    });
  });

  it('sends personal permissions only when a row uses them', () => {
    expect(
      toSetAccess([{ role: 'personal', projects: [7] }], logs).personal,
    ).toEqual({ permissions: logs, project_ids: [7] });
  });

  it('compares regardless of order', () => {
    const a = toSetAccess(
      [
        { role: 2, projects: null },
        { role: 3, projects: [7, 8] },
      ],
      [],
    );
    const b = toSetAccess(
      [
        { role: 3, projects: [8, 7] },
        { role: 2, projects: null },
      ],
      [],
    );
    expect(sameAccess(a, b)).toBe(true);
    expect(sameAccess(a, { ...b, assignments: [] })).toBe(false);
    expect(
      sameAccess(
        toSetAccess([{ role: 'personal', projects: null }], logs),
        toSetAccess([{ role: 'personal', projects: null }], []),
      ),
    ).toBe(false);
  });
});

describe('reachesProjects', () => {
  const catalog = [
    { key: 'storage', scope: 'instance' },
    { key: 'logs', scope: 'project' },
  ] as CatalogResource[];

  it('is true with any permission of project scope', () => {
    expect(reachesProjects(logs, catalog)).toBe(true);
    expect(
      reachesProjects([{ resource: 'storage', action: 'read' }], catalog),
    ).toBe(false);
    expect(reachesProjects([], catalog)).toBe(false);
  });
});
