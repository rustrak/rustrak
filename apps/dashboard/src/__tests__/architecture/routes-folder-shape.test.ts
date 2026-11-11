import { projectFiles } from 'archunit';
import { describe, expect, it } from 'vitest';
import { isTestFile, posix } from './predicates';

/**
 * Everything under `routes/` is either a route or a component in a
 * `-components/` folder.
 *
 * **The same rule the Next app had, against a different framework's contract.**
 * There it was "a Next special file or a `_components/` folder", and it existed
 * because sixteen components had accumulated loose beside their `page.tsx`,
 * where nothing could tell whether they belonged to that route or to the
 * feature whose type they rendered. The pressure is identical here and the
 * only thing that changed is the spelling: TanStack ignores a path segment
 * prefixed with `-`, so `-components/` is what `_components/` was.
 *
 * That spelling is not decoration. A file under `routes/` that is *not* in a
 * `-`-prefixed folder is a route: the generator reads the tree and mints one
 * from the filename. So a stray component here does not merely sit in the
 * wrong place, it silently becomes a URL.
 *
 * **Unconditional, with no size threshold.** One component beside a route is as
 * much a violation as eleven. A threshold is a judgement call, judgement calls
 * rot, and the six-component threshold this pattern replaced is what let the
 * sixteen accumulate.
 */

/** Anything inside a folder the route generator skips. */
const isIgnoredByTheGenerator = (path: string) =>
  posix(path)
    .split('/')
    .some((segment) => segment.startsWith('-'));

/**
 * The one file under `routes/` the generator writes rather than reads.
 * `__root.tsx` is the framework's own name for the root route, the way
 * `layout.tsx` was Next's.
 */
const isRootRoute = (path: string) =>
  posix(path).endsWith('/routes/__root.tsx');

describe('the shape of routes/', () => {
  /**
   * The population, asserted as a number rather than delegated to archunit.
   *
   * archunit raises `EmptyTestViolation` when a filter matches nothing, which
   * covers the total-glob-failure case. It does not report how many files it
   * did match, so a glob that silently narrowed to a handful would still pass
   * every negative below.
   */
  it('reads the population it expects to read', async () => {
    const underRoutes = await projectFiles()
      .inFolder('src/routes/**')
      .shouldNot()
      .adhereTo(() => true, 'counted')
      .check();

    // 36 route files and 16 route-local components: every screen the Next app
    // had, plus the two layouts the route groups used to provide implicitly.
    expect(underRoutes.length).toBeGreaterThanOrEqual(50);
  });

  /**
   * Every route file declares a route.
   *
   * This is the check the `-components/` convention makes possible: once the
   * components are out of the way, everything left must be a route, and a file
   * that forgot its `createFileRoute` is a page that 404s at runtime with
   * nothing at build time to say so.
   */
  it('has no file beside a route that is not one', async () => {
    const rule = projectFiles()
      .inFolder('src/routes/**')
      .shouldNot()
      .adhereTo(
        (file) =>
          !isTestFile(file.path) &&
          !isIgnoredByTheGenerator(file.path) &&
          !isRootRoute(file.path) &&
          !file.content.includes('createFileRoute'),
        'sits loose under routes/ without declaring a route: move it to a -components/ folder, or to the feature whose type it renders',
      );

    await expect(rule).toPassAsync();
  });

  /**
   * Route-local components stay local.
   *
   * A `-components/` folder is the composition seam for *one* route: its
   * contents name several features at once, which is exactly what disqualifies
   * them from living in any one of them. Importing one from another route
   * means it was never route-local, and the honest home is `shared/ui` (if its
   * props are primitives) or the feature whose type it renders.
   */
  it('has no route-local component imported from another route', async () => {
    const rule = projectFiles()
      .inFolder('src/routes/**')
      .shouldNot()
      .adhereTo((file) => {
        if (isTestFile(file.path) || isIgnoredByTheGenerator(file.path)) {
          return false;
        }

        // The importing file's own folder, so `./-components/x` and
        // `./thing/-components/x` are its own and anything reached by climbing
        // out with `../` is not.
        return /(?:from|import)\s+['"](?:\.\.\/)+[^'"]*\/-[^'"/]+\//.test(
          file.content,
        );
      }, 'imports a component out of another route\u2019s -components/ folder: it is not route-local, so move it down into shared/ui or the feature it names');

    await expect(rule).toPassAsync();
  });
});
