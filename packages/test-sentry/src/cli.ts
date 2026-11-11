#!/usr/bin/env node
import { parseArgs } from 'node:util';
import * as Sentry from '@sentry/node';
import { DSN } from './env.js';
import {
  initSentry,
  testAsyncError,
  testCapturedError,
  testCaptureMessage,
  testCustomError,
  testErrorFlood,
  testErrorLevels,
  testMultipleErrors,
  testNestedError,
  testReferenceError,
  testTypeError,
  testWithBreadcrumbs,
  testWithContexts,
  testWithExtraContext,
  testWithFingerprint,
  testWithTags,
  testWithTransaction,
  testWithUserContext,
} from './index.js';
import { testSourceMaps } from './sourcemaps.js';

type Options = {
  dsn: string;
  org: string;
  project: string;
  token?: string;
  floodCount: number;
};

/**
 * Every scenario, in `--all` order. Help, flag parsing and `--all` come from
 * here; `inAll: false` keeps one out of `--all`.
 */
const SCENARIOS: {
  flag: string;
  title: string;
  run: (options: Options) => unknown;
  inAll?: false;
}[] = [
  { flag: 'error', title: 'Basic captured error', run: testCapturedError },
  { flag: 'type-error', title: 'TypeError exception', run: testTypeError },
  {
    flag: 'ref-error',
    title: 'ReferenceError exception',
    run: testReferenceError,
  },
  { flag: 'custom-error', title: 'Custom error class', run: testCustomError },
  { flag: 'nested', title: 'Nested error with cause', run: testNestedError },
  { flag: 'async', title: 'Async stack trace', run: testAsyncError },
  {
    flag: 'message',
    title: 'Capture message (non-error)',
    run: testCaptureMessage,
  },
  {
    flag: 'breadcrumbs',
    title: 'Error with breadcrumbs',
    run: testWithBreadcrumbs,
  },
  { flag: 'user', title: 'Error with user context', run: testWithUserContext },
  { flag: 'tags', title: 'Error with custom tags', run: testWithTags },
  {
    flag: 'context',
    title: 'Error with extra context',
    run: testWithExtraContext,
  },
  {
    flag: 'contexts',
    title: 'Error with rich contexts (device, os, app)',
    run: testWithContexts,
  },
  {
    flag: 'fingerprint',
    title: 'Error with custom fingerprint',
    run: testWithFingerprint,
  },
  {
    flag: 'transaction',
    title: 'Error with transaction name',
    run: testWithTransaction,
  },
  { flag: 'levels', title: 'Every severity level', run: testErrorLevels },
  {
    flag: 'multiple',
    title: 'Multiple sequential errors',
    run: testMultipleErrors,
  },
  {
    flag: 'flood',
    title: 'Many errors: --flood [count], default 50',
    run: (o) => testErrorFlood(o.floodCount),
    inAll: false,
  },
  {
    flag: 'source-maps',
    title: 'Upload an artifact bundle and send a test event (needs --token)',
    run: (o) => {
      const dsn = new URL(o.dsn);
      return testSourceMaps({
        serverUrl: `${dsn.protocol}//${dsn.host}`,
        org: o.org,
        project: o.project,
        token: o.token!,
      });
    },
    inAll: false,
  },
];

const HELP = `
Usage: pnpm start [options]

Scenarios:
  --all               Every scenario below except flood and source-maps
${SCENARIOS.map((s) => `  --${s.flag.padEnd(18)}${s.title}`).join('\n')}

Options:
  --dsn <dsn>         Default: SENTRY_DSN, else ${DSN}
  --org <slug>        Source maps organization (default: my-org)
  --project <slug>    Source maps project (default: my-project)
  --token <token>     API token for source maps (or RUSTRAK_TOKEN)
  --env <env>         Environment (default: test)
  --release <rel>     Release
  --debug             Sentry SDK debug output
  --help
`;

function readOptions(args: string[]) {
  const { values, positionals } = parseArgs({
    args,
    allowPositionals: true,
    options: {
      ...Object.fromEntries(
        SCENARIOS.map((s) => [s.flag, { type: 'boolean' as const }]),
      ),
      all: { type: 'boolean' },
      help: { type: 'boolean', short: 'h' },
      debug: { type: 'boolean', default: false },
      dsn: { type: 'string', default: DSN },
      env: { type: 'string', default: 'test' },
      release: { type: 'string' },
      org: { type: 'string', default: 'my-org' },
      project: { type: 'string', default: 'my-project' },
      token: { type: 'string', default: process.env.RUSTRAK_TOKEN },
    },
  });
  return {
    ...values,
    dsn: values.dsn as string,
    env: values.env as string,
    org: values.org as string,
    project: values.project as string,
    token: values.token as string | undefined,
    release: values.release as string | undefined,
    debug: values.debug === true,
    // `--flood 100`: the count is the one positional.
    floodCount: Number(positionals[0] ?? 50),
    // `--all` adds to the flags named explicitly, never replaces them.
    selected: SCENARIOS.filter(
      (s) =>
        (values.all && s.inAll !== false) ||
        (values as Record<string, unknown>)[s.flag],
    ),
  };
}

const options = readOptions(process.argv.slice(2));

if (options.help || options.selected.length === 0) {
  console.log(HELP);
  process.exit(options.help ? 0 : 1);
}
if (
  options.selected.some((s) => s.flag === 'flood') &&
  !(Number.isSafeInteger(options.floodCount) && options.floodCount > 0)
) {
  console.error('--flood takes a positive whole number of events');
  process.exit(1);
}
if (options.selected.some((s) => s.flag === 'source-maps') && !options.token) {
  console.error('--source-maps needs --token <token> or RUSTRAK_TOKEN');
  process.exit(1);
}

console.log(`DSN: ${options.dsn}\nEnvironment: ${options.env}`);
initSentry({
  dsn: options.dsn,
  debug: options.debug,
  environment: options.env,
  release: options.release,
});

for (const scenario of options.selected) {
  console.log(`\n--- ${scenario.title} ---`);
  await scenario.run(options);
}

const flushed = await Sentry.flush(10_000);
console.log(`\nFlush ${flushed ? 'completed' : 'timed out'}`);
process.exit(flushed ? 0 : 1);
