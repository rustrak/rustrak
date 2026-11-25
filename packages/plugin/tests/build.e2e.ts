// End to end: `build` compiles a real plugin and packs it, needing nothing
// but cargo and node. Rustrak's own tests install what it packs.

import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { strFromU8, unzipSync } from 'fflate';
import { describe, expect, it } from 'vitest';
import { build } from '../src/build.ts';
import { scaffold } from '../src/new.ts';

const repo = resolve(import.meta.dirname, '../../..');
const sdk = join(repo, 'packages/rustrak-plugin');

/** A new project, built against this repo's SDK rather than crates.io's. */
function scaffoldHere(dir: string): void {
  scaffold(dir);
  const cargo = join(dir, 'Cargo.toml');
  writeFileSync(
    cargo,
    readFileSync(cargo, 'utf8').replace(
      /^rustrak-plugin = .*$/m,
      `rustrak-plugin = { path = ${JSON.stringify(sdk)} }`,
    ),
  );
}

describe('build', () => {
  it('packs the hello fixture, its routes inside its component', async () => {
    const out = mkdtempSync(join(tmpdir(), 'rustrak-plugin-out-'));
    const { rtp } = await build(
      join(repo, 'apps/server/tests/fixtures/hello-plugin'),
      { outDir: out },
    );

    expect(rtp).toBe(join(out, 'hello-0.1.0.rtp'));

    const files = unzipSync(readFileSync(rtp));
    expect(Object.keys(files).sort()).toEqual([
      'MANIFEST',
      'logo.svg',
      'messages/en.json',
      'messages/es.json',
      'migrations/0001_hits.sql',
      'migrations/0002_hit_times.sql',
      'migrations/0003_daily_hits.sql',
      'public/hello.js',
      'readme/en.md',
      'readme/es.md',
      'rustrak-plugin.toml',
      'server.wasm',
      'ui/index.css',
      'ui/index.js',
    ]);
    const manifest = strFromU8(files['rustrak-plugin.toml']!);
    expect(manifest).toContain('name = "Hello"');
    // The routes are in the component, where Rustrak reads them: kept
    // through the release profile's LTO and strip.
    const wasm = strFromU8(files['server.wasm']!, true);
    expect(wasm).toContain('rustrak:routes');
    expect(wasm).toContain('"path":"/greet"');
    expect(manifest).toContain('icon = "star"');
    expect(manifest).toContain('[[settings]]\nkey = "greeting"');
    expect(manifest).toContain(
      '{ path = "", scope = "project", sidebar = { label = "nav.hits", icon = "performance", rank = 1 } }',
    );
    expect(manifest).toContain('{ path = "hits/:hitId", scope = "project" }');
    expect(manifest).not.toContain('scope = "settings"');
    expect(manifest).toContain('widgets = [{ zone = "issue.details.after" }]');
    const bundle = strFromU8(files['ui/index.js']!);
    expect(bundle).toContain('__rustrakPlugins.register(`hello`');
    expect(bundle.length).toBeLessThan(20_000);
  });
});

describe('new', () => {
  it('scaffolds a plugin that builds and validates as it is', async () => {
    const dir = join(
      mkdtempSync(join(tmpdir(), 'rustrak-plugin-new-')),
      'my-plugin',
    );
    scaffoldHere(dir);

    for (const file of [
      'package.json',
      'rustrak.config.ts',
      'Cargo.toml',
      'src/server/routes/visits.rs',
      'src/server/models/visit.rs',
      'src/ui/routes/project/page.tsx',
      'src/ui/widgets/issue-visits.tsx',
      'src/messages/en.json',
      'src/messages/es.json',
      'logo.svg',
      'tsconfig.json',
      'migrations/0001_init.sql',
      'readme/en.md',
      'readme/es.md',
      '.gitignore',
    ]) {
      expect(existsSync(join(dir, file)), file).toBe(true);
    }

    // Cargo puts the component where its config says, not in target/.
    mkdirSync(join(dir, '.cargo'));
    writeFileSync(
      join(dir, '.cargo/config.toml'),
      '[build]\ntarget-dir = "elsewhere"\n',
    );
    const { rtp, manifest } = await build(dir, { outDir: join(dir, 'dist') });
    expect(existsSync(join(dir, 'elsewhere'))).toBe(true);
    expect(manifest).toContain('[ui]');
    const wasm = strFromU8(unzipSync(readFileSync(rtp))['server.wasm']!, true);
    expect(wasm).toContain('"path":"/visits"');
  });

  it('refuses to compile a route outside server/routes/', async () => {
    const dir = join(
      mkdtempSync(join(tmpdir(), 'rustrak-plugin-new-')),
      'misplaced',
    );
    scaffoldHere(dir);
    const model = join(dir, 'src/server/models/visit.rs');
    writeFileSync(
      model,
      `${readFileSync(model, 'utf8')}\n#[get("/oops")]\nfn oops() -> NoContent {\n    NoContent\n}\n`,
    );
    await expect(build(dir, { outDir: join(dir, 'dist') })).rejects.toThrow(
      /a route goes in src\/server\/routes\/ \(this is src\/server\/models\/visit\.rs\)/,
    );
  });

  it('refuses to write over a directory that has files', () => {
    const dir = mkdtempSync(join(tmpdir(), 'rustrak-plugin-new-'));
    scaffold(join(dir, 'ab'));
    expect(() => scaffold(join(dir, 'ab'))).toThrow(/not empty/);
  });
});
