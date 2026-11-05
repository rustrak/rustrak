import type { CatalogResource, Permission } from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import {
  grantable,
  hasPermission,
  modulesOf,
  rowState,
  toggle,
  toggleRow,
} from './permission-matrix';

const catalog: CatalogResource[] = [
  {
    key: 'issues',
    module: 'issues',
    scope: 'project',
    actions: ['read', 'update', 'delete'],
  },
  {
    key: 'storage',
    module: 'settings',
    scope: 'instance',
    actions: ['read', 'update', 'delete'],
  },
  { key: 'logs', module: 'monitoring', scope: 'project', actions: ['read'] },
];

describe('access model', () => {
  it('groups the catalog into modules in a fixed order', () => {
    expect(modulesOf(catalog).map((m) => m.module)).toEqual([
      'settings',
      'issues',
      'monitoring',
    ]);
  });

  it('toggles one cell', () => {
    const on = toggle([], 'issues', 'read');
    expect(hasPermission(on, 'issues', 'read')).toBe(true);
    expect(toggle(on, 'issues', 'read')).toEqual([]);
  });

  it('toggles a whole row, only over the actions it accepts', () => {
    const issues = catalog[0] as CatalogResource;
    const all = toggleRow([], issues);
    expect(all).toHaveLength(3);
    expect(rowState(all, issues)).toBe('all');
    expect(toggleRow(all, issues)).toEqual([]);
    const some: Permission[] = [{ resource: 'issues', action: 'read' }];
    expect(rowState(some, issues)).toBe('some');
    expect(rowState(toggleRow(some, issues), issues)).toBe('all');
  });
});

describe('grantable', () => {
  const catalog = [
    {
      key: 'storage',
      module: 'settings',
      scope: 'instance',
      actions: ['read', 'delete'],
    },
    { key: 'logs', module: 'monitoring', scope: 'project', actions: ['read'] },
  ] as CatalogResource[];

  it('keeps only what the reader holds, and drops what is left empty', () => {
    expect(
      grantable(catalog, {
        canAnywhere: (resource, action) =>
          resource === 'storage' && action === 'read',
      }),
    ).toEqual([{ ...catalog[0], actions: ['read'] }]);
  });

  it('keeps everything for someone who holds everything', () => {
    expect(grantable(catalog, { canAnywhere: () => true })).toEqual(catalog);
  });
});
