import MCR, { type CoverageReportOptions } from 'monocart-coverage-reports';

/**
 * How much of `apps/dashboard/src` the journeys execute, from Chromium's own
 * V8 coverage mapped back through the build's source maps. Only with
 * `E2E_COVERAGE=1` and a `vite build --sourcemap` build; CI uploads the lcov
 * to Codecov under the `dashboard` flag, which is the README badge.
 */
export const enabled = Boolean(process.env.E2E_COVERAGE);

export const options: CoverageReportOptions = {
  name: 'Dashboard e2e coverage',
  outputDir: 'coverage-e2e',
  reports: ['lcovonly', 'console-summary'],
  // The app's own chunks: not the service worker, the bundler's runtime or
  // the translation catalogues (`assets/en-*.js`), which are data.
  entryFilter: (entry) =>
    entry.url.includes('/assets/') &&
    !/rolldown-runtime|\/assets\/(en|es|fr|zh|ro)-/.test(entry.url),
  // Our TypeScript only: not `@rustrak/ui`, the client or the dependencies
  // (d3 ships its own `src/*.js`), which are tested where they live. Source
  // maps name files relative to `dist/assets`, so ours arrive as `src/...`.
  sourceFilter: (path) =>
    /(^|\/)src\/.+\.tsx?$/.test(path) && !/node_modules|packages\//.test(path),
  // A file no journey ever loads counts as 0%, not as absent.
  all: {
    dir: ['./src'],
    filter: {
      '**/*.test.ts': false,
      '**/__tests__/**': false,
      '**/routeTree.gen.ts': false,
      '**/*.{ts,tsx}': true,
    },
  },
  // Repository-relative, so Codecov can line it up with the files.
  sourcePath: (path) => {
    const at = path.indexOf('src/');
    return at === -1 ? path : `apps/dashboard/${path.slice(at)}`;
  },
};

export default async function generate() {
  if (enabled) await MCR(options).generate();
}
