import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

/**
 * Architecture rules and pure modules, in Node. Deliberately no jsdom, no
 * React plugin, no testing-library: nothing collected here renders.
 */
export default defineConfig({
  /**
   * Vite resolves `@/` through `vite.config.ts`; Vitest reads this file
   * instead, so the alias has to be stated in both. Type-only `@/` imports
   * vanish at compile time, so a module using nothing but those would run here
   * without one — and the first test whose subject imports a real *value*
   * fails to resolve, which reads as a missing package rather than as missing
   * config.
   */
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  test: {
    /**
     * Sequential files, one shared module registry, and it is a **20x
     * reduction in work** rather than a scheduling preference.
     *
     * archunit resolves the import graph by building a TypeScript program, and
     * caches it at module scope. Vitest's default is a fresh module registry
     * per test file, so nine rule files meant nine full extractions of the same
     * program. Sharing the registry means it is built once and the others read
     * the cache.
     *
     * Safe here because nothing in this suite has mutable state to leak: the
     * rules read files and assert. A future test that stubs a global would need
     * to opt back into isolation, and should say so where it does.
     */
    isolate: false,
    fileParallelism: false,
    // Node, not jsdom. Nothing here touches a DOM.
    environment: 'node',
    // `globals` is required by archunit's `toPassAsync` matcher, which the
    // rules assert with. Not a convenience.
    globals: true,
    // The architecture rules, plus unit tests for pure modules: a feature's
    // `model` and `lib`, `shared/{lib,i18n,api}`, and a route's own
    // `-components` helpers. Anywhere else, a `.test.ts` is not collected.
    include: [
      'src/__tests__/architecture/**/*.test.ts',
      'src/{features/*/{model,lib},shared/{lib,i18n,api}}/**/*.test.ts',
      'src/routes/**/-components/**/*.test.ts',
    ],
    exclude: ['**/node_modules/**', '**/dist/**'],
    // Whichever test runs first pays for the whole graph extraction; the rest
    // cost milliseconds. That one test needs room on a cold runner, and since
    // the suite finishes in seconds, a generous ceiling costs nothing and a
    // tight one costs a red build.
    testTimeout: 60_000,
    hookTimeout: 60_000,
  },
});
