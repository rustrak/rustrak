import { projectFiles } from 'archunit';
import { describe, expect, it } from 'vitest';
import { isMessages, isTestFile, withoutComments } from './predicates';

/**
 * Translating the chrome is not translating the dashboard: dates and numbers
 * are data, and they must read in the reader's locale too.
 *
 * A locale-blind formatter is worse than untranslated. `toLocaleString()` with no
 * argument resolves the locale **of whatever runtime runs it**, so the value
 * formats to the browser's locale rather than the one the reader chose, and
 * the timezone comes from the machine rather than from their account. That is
 * a correctness bug wearing a formatting bug's clothes.
 *
 * `use-intl`'s `useFormatter` reads the resolved locale and the configured
 * `timeZone`, memoises the `Intl` instances, and the option sets live once in
 * `shared/i18n/config.ts`. So the rule is a flat ban.
 *
 * **Why a content predicate and not `dependOnFiles`.** `date-fns` and `Intl`
 * are an external package and a global; neither is a node in the project's
 * import graph, so there is nothing for a graph rule to point at. `adhereTo` is
 * the API the library documents for the remaining case, and `portable-core`
 * already leans on it for the same reason.
 */

const LOCALE_BLIND_FORMAT =
  /toLocale(?:String|DateString|TimeString)\s*\(|new Intl\.|(?:from|import)\s+['"]date-fns['"]/;

/** Every file a rule here judges: source, not tests, not the dictionaries. */
const judged = (path: string) => !isTestFile(path) && !isMessages(path);

describe('the locale reaches the data, not just the chrome', () => {
  it('formats every date and number through the request locale', async () => {
    const rule = projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) =>
          judged(file.path) &&
          LOCALE_BLIND_FORMAT.test(withoutComments(file.content)),
        "formats a date or a number without the reader's locale: use use-intl's useFormatter",
      );

    await expect(rule).toPassAsync();
  });
});
