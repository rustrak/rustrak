// The manifest inside a `.rtp`: identity from package.json, the rest from
// rustrak.config and src/ui/. Authors never see or write it. What the code
// declares (its routes, its database) Rustrak reads from the component.

import { icons } from '@rustrak/ui';
import type { PluginConfig } from './config.ts';
import { permissionsManifest } from './permissions.ts';
import { settingsManifest } from './settings.ts';
import type { UiManifest } from './ui-build.ts';

/** The `rustrak:plugin` world this compiler and its SDK implement. */
export const WORLD = '0.1.0';

export interface PackageJson {
  name: string;
  version: string;
  description?: string;
  author?: string | { name?: string };
  repository?: string | { url?: string };
}

/** Rustrak cannot tell a design-system icon; the dashboard draws it. */
export function checkIcon(icon: string | undefined, where: string): void {
  if (icon !== undefined && !Object.hasOwn(icons, icon)) {
    throw new Error(`${where}: ${icon} is not an icon of @rustrak/ui`);
  }
}

/** The plugin's id: the config's, or the package name without scope or prefix. */
export function pluginId(packageName: string, explicit?: string): string {
  if (explicit) return explicit;
  const bare = packageName.replace(/^@[^/]+\//, '');
  return bare.replace(/^rustrak-plugin-/, '');
}

const key = (k: string) => (/^[A-Za-z0-9_-]+$/.test(k) ? k : JSON.stringify(k));

function value(v: unknown): string {
  if (typeof v === 'string') return JSON.stringify(v);
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  if (Array.isArray(v)) return `[${v.map(value).join(', ')}]`;
  if (v && typeof v === 'object') {
    const pairs = Object.entries(v)
      .filter(([, x]) => x != null)
      .map(([k, x]) => `${key(k)} = ${value(x)}`);
    return pairs.length ? `{ ${pairs.join(', ')} }` : '{}';
  }
  throw new Error(`cannot write ${String(v)} in the manifest`);
}

function table(header: string, entries: Record<string, unknown>): string {
  const lines = Object.entries(entries)
    .filter(([, v]) => v != null)
    .map(([k, v]) => `${key(k)} = ${value(v)}`);
  return `${header}\n${lines.join('\n')}\n`;
}

const text = (
  v: string | { name?: string; url?: string } | undefined,
  field: 'name' | 'url',
) => (typeof v === 'string' ? v : v?.[field]);

export function buildManifest({
  id,
  pkg,
  config,
  ui,
}: {
  id: string;
  pkg: PackageJson;
  config: PluginConfig;
  ui?: UiManifest;
}): string {
  checkIcon(config.icon, 'rustrak.config');
  const sections = [
    table('[plugin]', {
      id,
      name: config.name,
      version: pkg.version,
      description: pkg.description,
      rustrak: config.rustrak,
      world: WORLD,
      repository: text(pkg.repository, 'url')?.replace(/^git\+/, ''),
      author: text(pkg.author, 'name'),
      icon: config.icon,
    }),
  ];
  if (config.storageMb !== undefined)
    sections.push(table('[capabilities]', { storage_mb: config.storageMb }));
  for (const p of permissionsManifest(config.permissions ?? {}))
    sections.push(table('[[permissions]]', p));
  for (const s of settingsManifest(config.settings ?? {}))
    sections.push(table('[[settings]]', s));
  if (ui)
    sections.push(table('[ui]', { pages: ui.pages, widgets: ui.widgets }));
  return sections.join('\n');
}
