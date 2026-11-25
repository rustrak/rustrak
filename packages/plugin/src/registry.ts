// The registry's index: every package in a folder, by plugin, each version
// with the Rustrak versions it runs on, its URL (relative to the index) and
// its sha256. Rustrak trusts a package because the index names its hash and
// the index is signed: `minisign -Sm index.json`, in the registry's CI only.

import { createHash } from 'node:crypto';
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { strFromU8, unzipSync } from 'fflate';

interface Version {
  version: string;
  rustrak: string;
  url: string;
  sha256: string;
}

interface Listed {
  id: string;
  name: string;
  description?: string;
  icon?: string;
  author?: string;
  logo?: { url: string; sha256: string };
  versions: Version[];
}

/** The `[plugin]` table of a manifest `rustrak-plugin build` wrote. */
function pluginTable(toml: string): Record<string, string | string[]> {
  const table = toml.split(/^\[plugin\]$/m)[1]?.split(/^\[/m)[0] ?? '';
  const out: Record<string, string | string[]> = {};
  for (const line of table.split('\n')) {
    const m = line.match(/^(\w+) = (.+)$/);
    if (m) out[m[1]!] = JSON.parse(m[2]!) as string | string[];
  }
  return out;
}

/** Semver order, a pre-release before its release: `1.0.0-rc.1` < `1.0.0`. */
const older = (a: string, b: string) => {
  const [coreA = '', preA] = a.split('-', 2);
  const [coreB = '', preB] = b.split('-', 2);
  const [x, y] = [coreA.split('.').map(Number), coreB.split('.').map(Number)];
  for (let i = 0; i < 3; i++) {
    if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) - (y[i] ?? 0);
  }
  if (preA === preB) return 0;
  if (preA === undefined) return 1;
  if (preB === undefined) return -1;
  return preA.localeCompare(preB, 'en', { numeric: true });
};

/** Writes `<dir>/index.json` from the `.rtp` files in `dir`. */
export function writeIndex(dir: string): string {
  const plugins = new Map<string, Listed>();
  for (const file of readdirSync(dir)
    .filter((f) => f.endsWith('.rtp'))
    .sort()) {
    const bytes = readFileSync(join(dir, file));
    const files = unzipSync(bytes);
    const manifest = files['rustrak-plugin.toml'];
    if (!manifest) throw new Error(`${file} has no rustrak-plugin.toml`);
    const p = pluginTable(strFromU8(manifest));
    const id = String(p.id);
    const listed = plugins.get(id) ?? {
      id,
      name: '',
      versions: [],
    };
    listed.versions.push({
      version: String(p.version),
      rustrak: String(p.rustrak),
      url: file,
      sha256: createHash('sha256').update(bytes).digest('hex'),
    });
    listed.versions.sort((a, b) => older(a.version, b.version));
    // The newest version's name and description stand for the plugin.
    if (listed.versions.at(-1)?.url === file) {
      Object.assign(listed, {
        name: String(p.name),
        description: p.description as string | undefined,
        icon: p.icon as string | undefined,
        author: p.author as string | undefined,
      });
      // The newest version's logo, beside the index, named by the plugin's id.
      const logo = files['logo.svg'];
      if (logo) {
        writeFileSync(join(dir, `${id}.svg`), logo);
        listed.logo = {
          url: `${id}.svg`,
          sha256: createHash('sha256').update(logo).digest('hex'),
        };
      } else {
        delete listed.logo;
      }
    }
    plugins.set(id, listed);
  }
  const index = join(dir, 'index.json');
  writeFileSync(
    index,
    `${JSON.stringify({ version: 1, plugins: [...plugins.values()] }, null, 2)}\n`,
  );
  return index;
}
