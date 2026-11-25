// `rustrak-plugin build`: the compiler. Compiles server/ with locked settings,
// asks the Rustrak binary what the code declares, writes the manifest and
// packs everything into a `.rtp` the server accepts.

import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { strToU8, zipSync } from 'fflate';
import { filesIn, loadConfig, paths } from './config.ts';
import { generate } from './generate.ts';
import { buildManifest, type PackageJson, pluginId } from './manifest.ts';
import { missingLocales } from './messages.ts';
import { buildUi } from './ui-build.ts';

/** The release profile every plugin is built with, whatever its Cargo.toml says. */
const PROFILE: Record<string, string> = {
  CARGO_PROFILE_RELEASE_OPT_LEVEL: 's',
  CARGO_PROFILE_RELEASE_LTO: 'true',
  CARGO_PROFILE_RELEASE_STRIP: 'true',
  CARGO_PROFILE_RELEASE_CODEGEN_UNITS: '1',
  CARGO_PROFILE_RELEASE_PANIC: 'abort',
};

const TARGET = 'wasm32-wasip2';

/** A line of cargo's `--message-format=json`, as far as it is read here. */
interface Artifact {
  reason?: string;
  target?: { kind: string[] };
  filenames?: string[];
}

function run(
  cmd: string,
  args: string[],
  opts: { cwd?: string; env?: NodeJS.ProcessEnv } = {},
): string {
  try {
    return execFileSync(cmd, args, {
      cwd: opts.cwd,
      env: opts.env,
      encoding: 'utf8',
      maxBuffer: 64 * 1024 * 1024,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
  } catch (e) {
    const err = e as { code?: string; stderr?: string; message: string };
    if (err.code === 'ENOENT') throw new Error(`${cmd} was not found`);
    // What the tool said is the error: cargo names the file.
    throw new Error(err.stderr?.trim() || `${cmd} ${args.join(' ')} failed`);
  }
}

/**
 * Compiles server/ to a component and returns its path, as cargo reports it:
 * a `target-dir` in a Cargo config or a `[lib] name` moves the file.
 */
function compileServer(dir: string): string {
  const out = run(
    'cargo',
    [
      'build',
      '--release',
      '--target',
      TARGET,
      '--message-format=json-render-diagnostics',
    ],
    { cwd: dir, env: { ...process.env, ...PROFILE } },
  );
  const wasm = out
    .split('\n')
    .filter((line) => line.startsWith('{'))
    .map((line) => JSON.parse(line) as Artifact)
    // The library, not a binary the author added: both are a `.wasm`.
    .filter(
      (m) =>
        m.reason === 'compiler-artifact' && m.target?.kind.includes('cdylib'),
    )
    .flatMap((m) => m.filenames ?? [])
    .find((f) => f.endsWith('.wasm'));
  if (!wasm) throw new Error('cargo built no .wasm: is the crate a cdylib?');
  return wasm;
}

/** The `.rtp`: the files plus the MANIFEST of their sha256 hashes. */
function zip(files: Record<string, Uint8Array>): Uint8Array {
  const listing = Object.entries(files)
    .map(
      ([path, bytes]) =>
        `${createHash('sha256').update(bytes).digest('hex')}  ${path}`,
    )
    .sort();
  return zipSync({ ...files, MANIFEST: strToU8(`${listing.join('\n')}\n`) });
}

/** Every file under `root`, keyed `<prefix>/<its path>`; none if it is absent. */
function filesUnder(root: string, prefix: string): Record<string, Uint8Array> {
  return Object.fromEntries(
    filesIn(root).map((path) => [
      `${prefix}/${relative(root, path).split('\\').join('/')}`,
      readFileSync(path),
    ]),
  );
}

/**
 * Runs the migrations, in order, on a database that lives only in memory:
 * one that does not run fails here, not at install. Rustrak runs them in
 * the same SQLite.
 */
function checkMigrations(dir: string): void {
  const db = new DatabaseSync(':memory:');
  try {
    for (const file of filesIn(dir).sort()) {
      if (!file.endsWith('.sql')) continue;
      try {
        db.exec(readFileSync(file, 'utf8'));
      } catch (e) {
        throw new Error(
          `migrations/${relative(dir, file)}: ${(e as Error).message}`,
        );
      }
    }
  } finally {
    db.close();
  }
}

export async function build(
  dir: string,
  { outDir = join(dir, 'dist') } = {},
): Promise<{ rtp: string; manifest: string }> {
  const pkg = JSON.parse(
    readFileSync(join(dir, 'package.json'), 'utf8'),
  ) as PackageJson;
  const config = await loadConfig(dir);
  const where = paths(dir);
  // Rustrak refuses a package without it too; said here, before cargo runs.
  const readmes = filesUnder(where.readme, 'readme');
  if (!readmes['readme/en.md'])
    throw new Error(
      'A plugin needs readme/en.md: its page in the catalog, in English (readme/es.md and others are optional)',
    );

  const { warnings, messages } = generate(dir, config);
  checkMigrations(where.migrations);
  const wasm = compileServer(dir);
  const id = pluginId(pkg.name, config.id);
  const uiOut = join(where.generated, 'ui-dist');
  const ui = await buildUi(dir, id, uiOut, messages, config.permissions);
  const manifest = buildManifest({ id, pkg, config, ui });
  const readmeWarnings = missingLocales(
    (l) => `readme/${l}.md` in readmes,
    (l) => `readme/${l}.md`,
  );
  for (const warning of [...readmeWarnings, ...warnings])
    console.warn(`warning: ${warning}`);

  const files: Record<string, Uint8Array> = {
    'rustrak-plugin.toml': strToU8(manifest),
    ...readmes,
    'server.wasm': readFileSync(wasm),
    // What it hands out to anyone, like a tracker script a website loads.
    ...filesUnder(where.public, 'public'),
    ...filesUnder(where.migrations, 'migrations'),
    ...(ui ? filesUnder(uiOut, 'ui') : {}),
  };
  // Its own mark, wherever Rustrak shows it.
  if (existsSync(join(dir, 'logo.svg')))
    files['logo.svg'] = readFileSync(join(dir, 'logo.svg'));
  // The texts travel with the package: Rustrak labels the plugin's settings
  // and permissions with them, with or without a UI.
  for (const [locale, texts] of Object.entries(messages)) {
    files[`messages/${locale}.json`] = strToU8(JSON.stringify(texts));
  }

  mkdirSync(outDir, { recursive: true });
  const rtp = join(outDir, `${id}-${pkg.version}.rtp`);
  writeFileSync(rtp, zip(files));
  return { rtp, manifest };
}
