import { projectFiles } from 'archunit';
import { describe, expect, it } from 'vitest';
import { isTestFile, posix } from './predicates';

/**
 * Inside a `ui` segment, a file's **kind** decides its folder.
 *
 * `ui` is the segment that grows. Every other one has a natural ceiling -- a
 * slice has a handful of queries, a couple of model types -- but the view layer
 * accumulates components, then the hooks that drive them, then view-only
 * formatting helpers, then a store when client state outgrows `useState`. Left
 * flat, `ui` becomes the `lib/` this phase just dismantled: a folder that
 * groups by nothing.
 *
 * So the same rule that empties `app/` applies one level down. Nothing sits at
 * a `ui` segment root; every file is in a folder naming what it is.
 *
 * **Only the kinds that exist have folders.** Today that is `components`
 * everywhere and `hooks` in `shared/ui`, which holds the single hook in the
 * codebase. `utils` and `stores` are in the list below because the moment a
 * first file of that kind appears, the choice should be obvious rather than a
 * fresh argument -- but no empty folder is created to anticipate them.
 *
 * **Where hooks live, settled.** `use-mobile` sat in `shared/lib` and does not
 * belong there: it calls `useState` and `useEffect` and answers a question
 * about the viewport, which is the view layer by definition. Moving it leaves
 * `lib` meaning one thing -- pure logic, no React, no Next -- which is exactly
 * what [portable-core](./portable-core.test.ts) requires of it. A hook in `lib`
 * would have passed that rule, because React is not Next, while quietly making
 * the segment unportable in practice.
 */

const UI_KINDS = ['components', 'hooks', 'utils', 'stores'];

/** A file directly inside `…/ui/`, in no kind folder. */
const AT_UI_ROOT = /(^|\/)ui\/[^/]+\.tsx?$/;

/** A file inside `…/ui/<something>/`, whatever the something is. */
const UI_KIND_FOLDER = /(^|\/)ui\/([^/]+)\//;

describe('the shape of a ui segment', () => {
  /**
   * The population. Every assertion below is a negative, and a negative over an
   * empty set is the vacuous pass AD-9 exists to prevent -- archunit's own
   * `EmptyTestViolation` cannot help here, because the filter is `src/**`, which
   * always matches, and the narrowing happens inside the predicate.
   */
  it('reads the population it expects to read', async () => {
    const inUi = await projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) => !isTestFile(file.path) && posix(file.path).includes('/ui/'),
        'counted',
      )
      .check();

    // 96 files across twelve ui segments: 25 hand-written shared components,
    // the 21-file shadcn kit, one hook, and 49 across the eleven slices.
    expect(inUi.length).toBeGreaterThanOrEqual(96);
  });

  it('has no file sitting at a ui segment root', async () => {
    const rule = projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) => !isTestFile(file.path) && AT_UI_ROOT.test(posix(file.path)),
        'sits at the root of a ui segment: put it in components/, hooks/, utils/ or stores/',
      );

    await expect(rule).toPassAsync();
  });

  /**
   * The folder names are closed, not free-form. Two people inventing
   * `helpers/` and `utils/` for the same thing is how the segment stops meaning
   * anything, and it is the failure mode a rule about *placement* alone would
   * not catch.
   */
  it('has no kind folder outside the closed list', async () => {
    const rule = projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) => {
          if (isTestFile(file.path)) return false;
          const kind = UI_KIND_FOLDER.exec(posix(file.path))?.[2];
          return kind != null && !UI_KINDS.includes(kind);
        },
        `sits in a ui folder that is not one of: ${UI_KINDS.join(', ')}`,
      );

    await expect(rule).toPassAsync();
  });
});
