// What a plugin lets people do, declared in rustrak.config.ts: Rustrak lists
// it in Roles and Members to be granted like its own permissions, the
// routes require it as a typed `Perm`, and the browser asks `can()`.

import type { Permissions } from './config.ts';
import { pascal } from './settings.ts';

/** The manifest's `[[permissions]]` tables. */
export function permissionsManifest(
  permissions: Permissions,
): Record<string, unknown>[] {
  return Object.entries(permissions).map(([resource, p]) => ({
    resource,
    scope: p.scope,
    actions: p.actions,
  }));
}

/**
 * `Perm` for `requires = Perm::Hits.read()`: one type per resource, with a
 * method per action it declares, so an undeclared one does not compile.
 */
export function permissionsRust(permissions: Permissions): string {
  const resources = Object.entries(permissions).map(([resource, p]) =>
    [
      `    pub struct ${pascal(resource)};`,
      `    impl ${pascal(resource)} {`,
      ...p.actions.map(
        (a) =>
          `        pub const fn ${a}(self) -> &'static str {\n            "${resource}:${a}"\n        }`,
      ),
      '    }',
    ].join('\n'),
  );
  if (resources.length === 0) return '';
  return [
    '/// What people can be granted, in Roles and Members: `requires = Perm::X.read()`.',
    '#[allow(non_snake_case)]',
    'pub mod Perm {',
    resources.join('\n\n'),
    '}',
    '',
  ].join('\n');
}

/** `interface PluginPermissions` for `usePluginPermissions().can()`. */
export function permissionsTs(permissions: Permissions): string {
  const lines = Object.entries(permissions).map(
    ([resource, p]) =>
      `    ${JSON.stringify(resource).replace(/^"([a-z0-9]+)"$/, '$1')}: ${p.actions.map((a) => `'${a}'`).join(' | ')};`,
  );
  return ['  interface PluginPermissions {', ...lines, '  }'].join('\n');
}
