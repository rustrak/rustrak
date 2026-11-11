import { projectFiles } from 'archunit';
import { describe, expect, it } from 'vitest';
import { isTestFile, posix, withoutComments } from './predicates';

/**
 * The rule this whole phase exists to buy: **the domain does not know the
 * framework.**
 *
 * `features/*​/model`, `features/*​/lib` and `shared/lib` hold the logic that is
 * expensive to rewrite and cheap to keep -- status derivation, stack-trace
 * formatting, session-health thresholds, error copy. None of it should care
 * which framework renders it, and a migration off Next should leave all of it
 * untouched.
 *
 * Measured before this phase: 90 of 189 files already imported nothing from
 * `next/*`, and another 44 touched only `next/link` and `next/navigation`,
 * which any framework replaces with a shim. That 71% was an accident of how the
 * code happened to be written. This rule is what turns it into a property.
 *
 * Deliberately **not** applied to `ui` segments or to `routes/`. Components use
 * `useRouter` and `Link`, and route files are the coupled edge by design.
 *
 * **The rule now bans the router and React too, not only Next.** Swapping one
 * framework for another is exactly the event this was written for, and it is
 * the event that proves it: `model/` and `lib/` came through the migration
 * untouched, because nothing in them had ever heard of the framework. Leaving
 * the ban naming only the framework that just left would retire the rule at
 * the moment it was vindicated.
 *
 * **Why a content predicate and not `dependOnFiles`.** archunit's graph rules
 * describe edges between files *in this project*; `next` is an external
 * package, so there is no node to point at. `adhereTo` is the API for the
 * remaining case, and it is the one the library documents for exactly this.
 */

const FRAMEWORK_IMPORT =
  /(?:from|import)\s+['"](?:next(?:\/|['"])|@tanstack\/react-router|react(?:-dom)?['"])/;

const CORE = [
  /(^|\/)features\/[^/]+\/model\//,
  /(^|\/)features\/[^/]+\/lib\//,
  /(^|\/)shared\/lib\//,
];

const isCore = (path: string) => CORE.some((p) => p.test(posix(path)));

describe('the portable core does not import a framework', () => {
  /**
   * The floor, and the failure it exists to catch.
   *
   * Every assertion here is "no file does X". If the three globs above ever
   * stop matching -- a segment renamed, a slice restructured -- the rule checks
   * an empty set and reports success, which is the vacuous-rule failure AD-9
   * exists to prevent. archunit's own `EmptyTestViolation` does not help: the
   * filter is `src/**`, which always matches, and the narrowing happens inside
   * the predicate where the library cannot see it.
   */
  it('reads the population it expects to read', async () => {
    const core = await projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) => !isTestFile(file.path) && isCore(file.path),
        'counted',
      )
      .check();

    // 14 files after the phase-6 migration.
    expect(core.length).toBeGreaterThanOrEqual(14);
  });

  it('has no file importing a framework', async () => {
    const rule = projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) =>
          !isTestFile(file.path) &&
          isCore(file.path) &&
          // Comments are blanked first, so the paragraph above -- which names
          // `next/link` while explaining the rule -- does not trip it.
          FRAMEWORK_IMPORT.test(withoutComments(file.content)),
        'imports a framework inside the portable core, which is the coupling this rule removed',
      );

    await expect(rule).toPassAsync();
  });
});
