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

/** `1.0.0-rc.1+b` → core `[1, 0, 0]`, pre `['rc', '1']`; build metadata dropped. */
function semver(v: string) {
  const s = v.split('+')[0] ?? '';
  const dash = s.indexOf('-');
  return {
    core: (dash < 0 ? s : s.slice(0, dash)).split('.').map(Number),
    pre: dash < 0 ? [] : s.slice(dash + 1).split('.'),
  };
}

/** One pre-release identifier against another: numbers as numbers, below words. */
function identifier(p: string, q: string): number {
  const [m, n] = [/^\d+$/.test(p), /^\d+$/.test(q)];
  if (m && n) return Number(p) - Number(q);
  if (m !== n) return m ? -1 : 1;
  return p < q ? -1 : p > q ? 1 : 0;
}

/** Semver order, a pre-release before its release: `1.0.0-rc.1` < `1.0.0`. */
const older = (a: string, b: string) => {
  const [x, y] = [semver(a), semver(b)];
  for (let i = 0; i < 3; i++) {
    const d = (x.core[i] ?? 0) - (y.core[i] ?? 0);
    if (d) return d;
  }
  if (!x.pre.length || !y.pre.length) return y.pre.length - x.pre.length;
  const shared = Math.min(x.pre.length, y.pre.length);
  for (let i = 0; i < shared; i++) {
    const d = identifier(x.pre[i] ?? '', y.pre[i] ?? '');
    if (d) return d;
  }
  // `rc` < `rc.1`: the shorter one, all else equal, comes first.
  return x.pre.length - y.pre.length;
};

/** What Rustrak accepts as `plugin.id`; it also names files here. */
const PLUGIN_ID = /^[a-z][a-z0-9-]{1,31}$/;

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
    if (!PLUGIN_ID.test(id))
      throw new Error(`${file}: ${JSON.stringify(id)} is not a plugin id`);
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
