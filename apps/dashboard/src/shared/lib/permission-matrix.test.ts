import type {
  Catalog,
  CatalogResource,
  Permission,
  Resource,
} from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import {
  editorCatalog,
  grantable,
  hasPermission,
  joinPermissions,
  modulesOf,
  rowState,
  splitPermissions,
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

  it('keeps a grant the narrowed row does not show', () => {
    const shown = { ...catalog[0], actions: ['read'] } as CatalogResource;
    const hidden: Permission = { resource: 'issues', action: 'delete' };
    expect(toggleRow([hidden], shown)).toContainEqual(hidden);
    expect(
      toggleRow([hidden, { resource: 'issues', action: 'read' }], shown),
    ).toEqual([hidden]);
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

describe("plugins' permissions in the editor", () => {
  const catalog = {
    resources: [
      { key: 'issues', module: 'issues', scope: 'project', actions: ['read'] },
    ],
    plugins: [
      {
        key: 'hello:hits',
        plugin: 'hello',
        plugin_name: 'Hello',
        scope: 'project',
        actions: ['read', 'delete'],
        label: { en: 'Hits', es: 'Visitas' },
        description: { en: 'See each hit.' },
      },
    ],
  } as const satisfies Catalog;

  it('lists them beside Rustrak’s own, one module per plugin', () => {
    const resources = editorCatalog(catalog);
    expect(resources.map((r) => [r.key, r.module])).toEqual([
      ['issues', 'issues'],
      ['hello:hits', 'plugin:hello'],
    ]);
    expect(resources[1]).toMatchObject({
      moduleName: 'Hello',
      label: { es: 'Visitas' },
    });
  });

  it('splits them back apart for the API', () => {
    expect(
      splitPermissions([
        { resource: 'issues', action: 'read' },
        { resource: 'hello:hits' as Resource, action: 'delete' },
      ]),
    ).toEqual({
      permissions: [{ resource: 'issues', action: 'read' }],
      plugin_permissions: [{ resource: 'hello:hits', action: 'delete' }],
    });
  });

  it('joins what the API keeps apart', () => {
    expect(
      joinPermissions(
        [{ resource: 'issues', action: 'read' }],
        [{ resource: 'hello:hits', action: 'read' }],
      ),
    ).toEqual([
      { resource: 'issues', action: 'read' },
      { resource: 'hello:hits', action: 'read' },
    ]);
  });
});
