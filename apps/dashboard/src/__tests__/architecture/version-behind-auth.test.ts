import { projectFiles } from 'archunit';
import { describe, expect, it } from 'vitest';
import { isTestFile, posix, withoutComments } from './predicates';

/**
 * The running version is told to signed-in people only.
 *
 * A version number is the first thing worth knowing about a host you intend to
 * attack: it turns "is this instance vulnerable to X" from a probe into a
 * lookup. The server stopped answering `/health/version` to anonymous callers,
 * and this rule is the other half of that change -- without it the number goes
 * straight back onto the login screen the next time someone wants a footer.
 *
 * **The boundary is `routes/_authenticated/`, because that is where the gate
 * is.** `_authenticated.tsx` redirects an anonymous session to `/login` in
 * `beforeLoad`, before it renders any child, so a page below it has a user by
 * construction. Everything else in `routes/` -- the login, the invitation --
 * is reachable with no session at all, and so is every component in `shared/`
 * that those pages render.
 *
 * Known limit, stated rather than papered over: `_authenticated.tsx` also has
 * an `unavailable` branch that renders `OutageScreen` without a user. A version
 * printed by a component that only that branch reaches would satisfy this rule
 * and still be visible to a stranger. `OutageScreen` lives in `shared/`, so it
 * is caught today; the rule is an approximation of the gate, not a proof of it.
 *
 * **Why a content predicate and not `dependOnFiles`.** The leak is a *rendered
 * string*, and the import is only its most likely shape. Matching the
 * identifier catches a re-export or a helper that reads `packageJson.version`
 * under another name being pulled in, which an edge between two named files
 * would not.
 */

const APP_VERSION = /\bAPP_VERSION\b/;

/**
 * Where the constant is declared. It has to name itself, and a module in
 * `shared/config` that only re-exports a number renders nothing on its own.
 */
const isDeclaration = (path: string) =>
  posix(path).endsWith('src/shared/config/constants.ts');

/** Below the redirect in `_authenticated.tsx`. */
const isBehindTheGate = (path: string) =>
  posix(path).includes('/routes/_authenticated/');

describe('the version is only rendered behind the auth gate', () => {
  /**
   * The floor. Every assertion below is "no file does X", so if the identifier
   * is ever renamed the rule would judge an empty set and pass while saying
   * nothing -- the vacuous-rule failure the suite exists to avoid.
   */
  it('reads the population it expects to read', async () => {
    const users = await projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) =>
          !isTestFile(file.path) &&
          !isDeclaration(file.path) &&
          APP_VERSION.test(withoutComments(file.content)),
        'counted',
      )
      .check();

    // The About page, at minimum.
    expect(users.length).toBeGreaterThanOrEqual(1);
  });

  it('has no file outside the gate naming APP_VERSION', async () => {
    const rule = projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) =>
          !isTestFile(file.path) &&
          !isDeclaration(file.path) &&
          !isBehindTheGate(file.path) &&
          // Comments are blanked first, so the paragraphs above -- which name
          // the constant while explaining the rule -- do not trip it.
          APP_VERSION.test(withoutComments(file.content)),
        'shows the instance version on a surface a signed-out visitor can reach',
      );

    await expect(rule).toPassAsync();
  });
});
