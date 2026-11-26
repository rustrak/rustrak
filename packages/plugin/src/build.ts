// `rustrak-plugin build`: the compiler. Compiles server/ with locked settings,
// builds the pages, writes the manifest and packs everything into a `.rtp` the server accepts.

import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  existsSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  writeFileSync,
} from 'node:fs';
import { join, relative } from 'node:path';
import { createInterface } from 'node:readline';
import { DatabaseSync } from 'node:sqlite';
import { gzipSync } from 'node:zlib';
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
  manifest_path?: string;
  target?: { kind: string[] };
  filenames?: string[];
}

/**
 * Runs cargo, handing each line it prints to `onOutput` as it comes; what it
 * wrote to stdout. On failure, what cargo said is the error: it names the file.
 */
function cargo(
  args: string[],
  opts: {
    cwd: string;
    env: NodeJS.ProcessEnv;
    onOutput?: (line: string) => void;
  },
): Promise<string> {
  return new Promise((done, fail) => {
    const child = spawn('cargo', args, {
      cwd: opts.cwd,
      env: opts.env,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    child.stdout.setEncoding('utf8').on('data', (chunk) => {
      stdout += chunk;
    });
    createInterface({ input: child.stderr }).on('line', (line) => {
      stderr += `${line}\n`;
      opts.onOutput?.(line);
    });
    child.on('error', (e: NodeJS.ErrnoException) =>
      fail(e.code === 'ENOENT' ? new Error('cargo was not found') : e),
    );
    child.on('close', (code) =>
      code === 0
        ? done(stdout)
        : fail(new Error(stderr.trim() || `cargo ${args.join(' ')} failed`)),
    );
  });
}

/**
 * Compiles server/ to a component and returns its path, as cargo reports it:
 * a `target-dir` in a Cargo config or a `[lib] name` moves the file.
 */
async function compileServer(
  dir: string,
  onOutput?: (line: string) => void,
): Promise<string> {
  const out = await cargo(
    [
      'build',
      '--release',
      '--target',
      TARGET,
      '--message-format=json-render-diagnostics',
    ],
    { cwd: dir, env: { ...process.env, ...PROFILE }, onOutput },
  );
  // Real paths both: cargo reports /private/var for macOS's /var.
  const manifest = realpathSync(join(dir, 'Cargo.toml'));
  const wasm = out
    .split('\n')
    .filter((line) => line.startsWith('{'))
    .map((line) => JSON.parse(line) as Artifact)
    // The plugin's own library: not a binary the author added, nor a
    // dependency that is a cdylib too. All of them are a `.wasm`.
    .filter(
      (m) =>
        m.reason === 'compiler-artifact' &&
        m.target?.kind.includes('cdylib') &&
        m.manifest_path !== undefined &&
        realpathSync(m.manifest_path) === manifest,
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
 * the same SQLite. How many there are.
 */
function checkMigrations(dir: string): number {
  const db = new DatabaseSync(':memory:');
  const sql = filesIn(dir)
    .filter((file) => file.endsWith('.sql'))
    .sort();
  try {
    for (const file of sql) {
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
  return sql.length;
}

/** What `build` is doing, in the order it does it. */
export type BuildStep = 'check' | 'server' | 'ui' | 'pack';

export interface BuildOptions {
  outDir?: string;
  /** A step starts; the one before it is done. */
  onStep?: (step: BuildStep) => void;
  /** A line cargo printed while compiling the server. */
  onOutput?: (line: string) => void;
}

/** What went into the `.rtp`, for whoever reports the build. */
export interface BuildSummary {
  id: string;
  version: string;
  /** Of the `.rtp`, in bytes. */
  size: number;
  /** In the `.rtp`, its MANIFEST left out. */
  files: number;
  /** Of `server.wasm`, in bytes. */
  server: number;
  migrations: number;
  pages: number;
  widgets: number;
  /** Every file in the `.rtp`, by path, with its size and gzipped size. */
  entries: { path: string; size: number; gzip: number }[];
}

export async function build(
  dir: string,
  { outDir = join(dir, 'dist'), onStep, onOutput }: BuildOptions = {},
): Promise<{
  rtp: string;
  manifest: string;
  warnings: string[];
  summary: BuildSummary;
}> {
  onStep?.('check');
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

  const generated = generate(dir, config);
  const { messages } = generated;
  const migrations = checkMigrations(where.migrations);

  onStep?.('server');
  const wasm = readFileSync(await compileServer(dir, onOutput));

  onStep?.('ui');
  const id = pluginId(pkg.name, config.id);
  const uiOut = join(where.generated, 'ui-dist');
  const ui = await buildUi(dir, id, uiOut, messages, config.permissions);

  onStep?.('pack');
  const manifest = buildManifest({ id, pkg, config, ui });
  const warnings = [
    ...missingLocales(
      (l) => `readme/${l}.md` in readmes,
      (l) => `readme/${l}.md`,
    ),
    ...generated.warnings,
  ];

  const files: Record<string, Uint8Array> = {
    'rustrak-plugin.toml': strToU8(manifest),
    ...readmes,
    'server.wasm': wasm,
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
  const packed = zip(files);
  writeFileSync(rtp, packed);
  return {
    rtp,
    manifest,
    warnings,
    summary: {
      id,
      version: pkg.version,
      size: packed.length,
      files: Object.keys(files).length,
      server: wasm.length,
      migrations,
      pages: ui?.pages.length ?? 0,
      widgets: ui?.widgets.length ?? 0,
      entries: Object.entries(files)
        .map(([path, data]) => ({
          path,
          size: data.length,
          gzip: gzipSync(data).length,
        }))
        .sort((a, b) => a.path.localeCompare(b.path)),
    },
  };
}
