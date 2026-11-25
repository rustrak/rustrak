import { describe, expect, it } from 'vitest';
import type { Permissions } from '../src/config.ts';
import { permissionsRust, permissionsTs } from '../src/permissions.ts';

const permissions: Permissions = {
  hits: { scope: 'project', actions: ['read', 'delete'] },
  stats: { scope: 'project', actions: ['read'] },
  reports: { scope: 'instance', actions: ['read', 'create'] },
};

describe('permissionsRust', () => {
  it('derives the typed Perm the routes require', () => {
    const rs = permissionsRust(permissions);
    expect(rs).toContain('pub mod Perm {');
    expect(rs).toContain('pub struct Hits;');
    expect(rs).toContain(
      'pub const fn delete(self) -> &\'static str {\n            "hits:delete"',
    );
    expect(rs).not.toContain('"hits:create"');
  });

  it('is nothing for a plugin with no permissions', () => {
    expect(permissionsRust({})).toBe('');
  });
});

describe('permissionsTs', () => {
  it("types the browser's can(resource, action)", () => {
    expect(permissionsTs(permissions)).toContain(
      "    hits: 'read' | 'delete';",
    );
  });
});
