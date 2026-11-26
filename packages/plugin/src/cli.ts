#!/usr/bin/env node
// rustrak-plugin: new | generate | build | index

import { execFileSync, spawn } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { parseArgs, stripVTControlCharacters, styleText } from 'node:util';
import {
  cancel,
  confirm,
  isCancel,
  isCI,
  text,
  updateSettings,
} from '@clack/prompts';
import { type BuildStep, type BuildSummary, build } from './build.ts';
import { loadConfig } from './config.ts';
import { generate } from './generate.ts';
import { nameError, RELEASE, scaffold } from './new.ts';
import { writeIndex } from './registry.ts';
import {
  bytes,
  crate,
  duration,
  groupWarnings,
  lime,
  logo,
  packageManager,
  redraws,
  runScript,
  stack,
  Trace,
} from './term.ts';

const TARGET = 'wasm32-wasip2';
const dim = (s: string) => styleText('dim', s);
const bold = (s: string) => styleText('bold', s);
const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;

// Colour is for severity, as in the dashboard: the rest is grey and lime.
const ok = (s: string) => console.log(`  ${styleText('green', '✓')} ${s}`);
const warn = (s: string) => console.log(`  ${styleText('yellow', '▲')} ${s}`);
const fail = (s: string) => console.log(`  ${styleText('red', '✗')} ${s}`);

const USAGE = `${lime(bold('rustrak-plugin'))} ${dim(`v${RELEASE}`)}

  ${bold('new')} [dir]      create a plugin project
               ${dim('--install / --no-install   install its dependencies')}
               ${dim('--git / --no-git           start a git repository')}
               ${dim('--yes                      take every default, ask nothing')}
  ${bold('generate')}       write .rustrak/ from rustrak.config.ts and src/messages/
               ${dim('(types for the editor, Settings for Rust)')}
  ${bold('build')}          compile and pack it into dist/<id>-<version>.rtp
  ${bold('index')} <dir>    the registry's index.json from the .rtp files in <dir>
               ${dim('(sign it: minisign -Sm <dir>/index.json)')}`;

/** A prompt's answer; Ctrl-C ends the command, saying so. */
async function ask<T>(
  answer: Promise<T | symbol>,
): Promise<Exclude<T, symbol>> {
  const value = await answer;
  if (isCancel(value)) {
    cancel('Cancelled. Nothing was written.');
    process.exit(0);
  }
  return value as Exclude<T, symbol>;
}

/**
 * Runs a command as a step of `trace`, its last line as the step's detail.
 * Its whole output when it fails, to print once the trace is closed.
 */
function run(
  trace: Trace,
  [doing, done]: [string, string],
  cmd: string,
  args: string[],
  cwd: string,
): Promise<string | undefined> {
  trace.begin(doing, done);
  const output: string[] = [];
  return new Promise((settle) => {
    // On Windows npm, pnpm and yarn are .cmd shims, which Node launches only
    // through a shell (CVE-2024-27980). The command is ours, never input.
    const child =
      process.platform === 'win32'
        ? spawn([cmd, ...args].join(' '), {
            cwd,
            shell: true,
            stdio: ['ignore', 'pipe', 'pipe'],
          })
        : spawn(cmd, args, { cwd, stdio: ['ignore', 'pipe', 'pipe'] });
    for (const stream of [child.stdout, child.stderr]) {
      createInterface({ input: stream }).on('line', (line) => {
        const plain = stripVTControlCharacters(line).trim();
        if (!plain) return;
        output.push(line);
        trace.detail(plain.slice(0, 60));
      });
    }
    child.on('error', (e) => {
      trace.fail();
      settle(e.message);
    });
    child.on('close', (code) => {
      if (code === 0) {
        trace.end('');
        settle(undefined);
      } else {
        trace.fail();
        settle(output.join('\n') || `${cmd} ${args.join(' ')} failed`);
      }
    });
  });
}

/** Whether `dir` sits in a git repository; `undefined` without git. */
function inGitRepo(dir: string): boolean | undefined {
  let at = dir;
  while (!existsSync(at)) at = dirname(at);
  try {
    execFileSync('git', ['rev-parse', '--is-inside-work-tree'], {
      cwd: at,
      stdio: 'ignore',
    });
    return true;
  } catch (e) {
    return (e as NodeJS.ErrnoException).code === 'ENOENT' ? undefined : false;
  }
}

/**
 * Whether rustup has the target plugins compile to, for the toolchain `dir`
 * selects (a rust-toolchain.toml above it); `undefined` without rustup.
 */
function hasTarget(dir: string): boolean | undefined {
  let at = dir;
  while (!existsSync(at)) at = dirname(at);
  try {
    return execFileSync('rustup', ['target', 'list', '--installed'], {
      cwd: at,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    })
      .split('\n')
      .includes(TARGET);
  } catch {
    return undefined;
  }
}

/** What `new` was told, by flags or by answering. */
interface Plan {
  dir: string;
  install: boolean;
  git: boolean;
  /** rustup has the target; `undefined` without rustup. */
  target: boolean | undefined;
  addTarget: boolean;
}

/** Asks what the flags left open; in CI or with --yes, takes the flags' word. */
async function plan(argv: string[], pm: string): Promise<Plan | undefined> {
  const { values, positionals } = parseArgs({
    args: argv,
    allowPositionals: true,
    allowNegative: true,
    options: {
      install: { type: 'boolean' },
      git: { type: 'boolean' },
      yes: { type: 'boolean', short: 'y' },
    },
  });
  const interactive =
    Boolean(process.stdin.isTTY && process.stdout.isTTY) &&
    !isCI() &&
    !values.yes;
  const choose = async (flag: boolean | undefined, message: string) =>
    flag ??
    (interactive ? await ask(confirm({ message })) : Boolean(values.yes));

  const arg =
    positionals[0] ??
    (interactive
      ? await ask(
          text({
            message: 'Where should the plugin go?',
            placeholder: './my-plugin',
            validate: (v) =>
              v ? nameError(resolve(v)) : 'A directory, named as the plugin',
          }),
        )
      : undefined);
  if (!arg) {
    fail('Name the directory: rustrak-plugin new <dir>');
    return undefined;
  }
  const dir = resolve(arg);
  const error = nameError(dir);
  if (error) {
    fail(error);
    return undefined;
  }
  const install = await choose(
    values.install,
    `Install dependencies with ${pm}?`,
  );
  // Never a repository inside another, nor one without git.
  const git =
    inGitRepo(dir) === false &&
    (await choose(values.git, 'Start a git repository?'));
  const target = hasTarget(dir);
  const addTarget =
    target === false &&
    (await choose(
      undefined,
      `Add the ${TARGET} target? Plugins compile to it.`,
    ));
  return { dir, install, git, target, addTarget };
}

/** git init and a first commit; a missing identity only skips the commit. */
function startRepo(trace: Trace, dir: string): void {
  trace.begin('Starting', 'Started');
  try {
    execFileSync('git', ['init', '--quiet'], { cwd: dir, stdio: 'ignore' });
  } catch {
    trace.fail();
    return;
  }
  try {
    for (const args of [
      ['add', '-A'],
      ['commit', '--quiet', '-m', 'Initial commit from rustrak-plugin'],
    ]) {
      execFileSync('git', args, { cwd: dir, stdio: 'ignore' });
    }
    trace.end('git repository, first commit');
  } catch {
    trace.end('git repository, no commit: git has no identity');
  }
}

/** What a new project holds, and what each part is for. */
const TREE: [string, string][] = [
  ['rustrak.config.ts', 'name, settings, permissions'],
  ['src/server/', 'routes and models, in Rust'],
  ['src/ui/', 'pages and widgets, in React'],
  ['src/messages/', 'texts, by language'],
  ['migrations/', "the plugin's own database"],
  ['readme/', 'its page in the catalog'],
];

async function create(argv: string[]): Promise<number> {
  await logo(`plugin kit v${RELEASE}`);
  // The prompts without clack's rail: the trace below has its own column.
  updateSettings({ withGuide: false });
  const pm = packageManager(process.env.npm_config_user_agent);
  const answers = await plan(argv, pm);
  if (!answers) return 1;
  const { dir, target } = answers;
  const shown = relative(process.cwd(), dir) || '.';
  const slug = basename(dir);

  console.log();
  const trace = new Trace();
  trace.begin('Creating', 'Created');
  const files = scaffold(dir);
  trace.end(`${shown}/ · ${plural(files, 'file')}`);
  const failures: string[] = [];
  let installed = false;
  if (answers.install) {
    const output = await run(
      trace,
      ['Installing', 'Installed'],
      pm,
      ['install'],
      dir,
    );
    installed = output === undefined;
    if (output) failures.push(`${pm} install failed:\n${output}`);
  }
  if (answers.addTarget) {
    const output = await run(
      trace,
      ['Adding', 'Added'],
      'rustup',
      ['target', 'add', TARGET],
      dir,
    );
    if (output) failures.push(output);
  }
  if (answers.git) startRepo(trace, dir);
  trace.close();

  console.log();
  for (const failure of failures) fail(failure);
  if (target === undefined) {
    warn(
      `Rust was not found. Install it from https://rustup.rs, then: rustup target add ${TARGET}`,
    );
  } else if (target === false && !answers.addTarget) {
    warn(`Plugins compile to ${TARGET}: rustup target add ${TARGET}`);
  }

  const width = Math.max(...TREE.map(([path]) => path.length));
  console.log(`  ${lime('●')} ${bold(`${shown}/`)}`);
  TREE.forEach(([path, what], i) => {
    const branch = i === TREE.length - 1 ? '└─' : '├─';
    console.log(`    ${dim(branch)} ${path.padEnd(width)}  ${dim(what)}`);
  });
  const steps = [`cd ${shown}`];
  if (!installed) steps.push(`${pm} install`);
  steps.push(runScript(pm, 'build'));
  console.log(`\n  ${dim('next')}  ${bold(steps.join(' && '))}`);
  console.log(
    `        ${dim(`then upload dist/${slug}-0.1.0.rtp in Settings › Plugins`)}\n`,
  );
  // The project is there either way; what was asked for and failed is not.
  return failures.length > 0 ? 1 : 0;
}

const VERBS: Record<BuildStep, [doing: string, done: string]> = {
  check: ['Checking', 'Checked'],
  server: ['Compiling', 'Compiled'],
  ui: ['Building', 'Built'],
  pack: ['Packing', 'Packed'],
};

/** The package's weight by part, as one bar and its legend. */
function weight(entries: BuildSummary['entries']): string[] {
  const of = (test: (path: string) => boolean) =>
    entries.filter((e) => test(e.path)).reduce((n, e) => n + e.gzip, 0);
  const server = of((p) => p === 'server.wasm');
  const ui = of((p) => p.startsWith('ui/'));
  const rest = of((p) => p !== 'server.wasm' && !p.startsWith('ui/'));
  const parts: [string, string, number][] = [
    ['█', 'server', server],
    ['▓', 'ui', ui],
    ['░', 'rest', rest],
  ];
  return [
    stack(
      parts.map(([, , n]) => n),
      36,
    ),
    parts
      .filter(([, , n]) => n > 0)
      .map(([glyph, name, n]) => `${glyph} ${dim(name)} ${bytes(n)}`)
      .join('   '),
  ];
}

/** `name version` from the project's package.json, for the header. */
function project(dir: string): string {
  try {
    const pkg = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'));
    return `${pkg.name} ${pkg.version}`;
  } catch {
    return basename(dir);
  }
}

async function runBuild(): Promise<number> {
  const dir = process.cwd();
  // cargo's diagnostics in its own colours, where they can be seen.
  if (redraws()) process.env.CARGO_TERM_COLOR ??= 'always';
  console.log(
    `\n  ${lime(bold('rustrak'))} build ${dim(`· ${project(dir)}`)}\n`,
  );
  const trace = new Trace();
  let crates = 0;

  try {
    const { rtp, warnings, summary } = await build(dir, {
      onStep: (step) => trace.begin(...VERBS[step]),
      onOutput: (line) => {
        const name = crate(line);
        if (name) crates++;
        // `Updating crates.io index`, `Downloaded 12 crates`: said as it is.
        trace.detail(
          name
            ? `(${crates}) ${name}`
            : stripVTControlCharacters(line).trim().slice(0, 60),
        );
      },
    });
    trace.end();
    trace.details([
      plural(summary.migrations, 'migration'),
      `${crates ? `${plural(crates, 'crate')} → ` : ''}${bytes(summary.server)}`,
      summary.pages + summary.widgets === 0
        ? 'no pages'
        : `${plural(summary.pages, 'page')}, ${plural(summary.widgets, 'widget')}`,
      plural(summary.files, 'file'),
    ]);
    const took = trace.close();

    const [bar, legend] = weight(summary.entries);
    console.log(
      `\n  ${lime('●')} ${bold(relative(dir, rtp))}  ${dim(bytes(summary.size))}`,
    );
    console.log(`    ${bar}\n    ${legend}\n`);
    for (const warning of groupWarnings(warnings)) warn(warning);
    ok(
      `built in ${duration(took)} ${dim('·')} upload it in ${bold('Settings › Plugins')}\n`,
    );
    return 0;
  } catch (e) {
    // The error itself follows, whole: not cut short in the step's line.
    trace.detail('');
    trace.fail();
    const took = trace.close();
    // What cargo or the check said, as they said it: it names the file.
    console.error(`\n${(e as Error).message}\n`);
    fail(`build failed after ${duration(took)}\n`);
    return 1;
  }
}

async function main(argv: string[]): Promise<number> {
  const [command, ...rest] = argv;
  if (command === 'new') return create(rest);
  if (command === 'build') return runBuild();
  if (command === 'generate') {
    const { warnings } = generate(
      process.cwd(),
      await loadConfig(process.cwd()),
    );
    for (const warning of groupWarnings(warnings)) warn(warning);
    ok('wrote .rustrak/');
    return 0;
  }
  if (command === 'index' && rest[0]) {
    ok(`wrote ${writeIndex(resolve(rest[0]))}`);
    return 0;
  }
  if (command === '--version' || command === '-v') {
    console.log(RELEASE);
    return 0;
  }
  console.error(USAGE);
  return command === undefined || command === '--help' || command === '-h'
    ? 0
    : 1;
}

main(process.argv.slice(2)).then(
  (code) => process.exit(code),
  (e: Error) => {
    fail(e.message);
    process.exit(1);
  },
);
