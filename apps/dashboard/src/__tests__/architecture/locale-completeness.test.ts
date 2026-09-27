import { projectFiles } from 'archunit';
import { describe, expect, it } from 'vitest';
import { isTestFile, withoutComments } from './predicates';

/**
 * Translating the chrome is not translating the dashboard.
 *
 * The i18n pass moved every sentence into `messages/*.json` and stopped there,
 * which left the half of the screen that is *data* still rendered in English:
 * 32 files formatted a date through `date-fns` with no `locale`, a number
 * through a bare `toLocaleString()`, or through an `Intl.NumberFormat('en')`
 * pinned at module scope. A viewer on `/zh` read "3 hours ago" under a Chinese
 * heading.
 *
 * Two of those three are worse than untranslated. `toLocaleString()` with no
 * argument resolves the locale **of whatever runtime runs it**, so the value
 * formats to the browser's locale rather than the one the reader chose, and
 * the timezone comes from the machine rather than from their account. That is
 * a correctness bug wearing a formatting bug's clothes.
 *
 * `use-intl`'s `useFormatter` answers all three: it reads the resolved locale
 * and the configured `timeZone`, it memoises the `Intl` instances the old
 * module-scope cache existed to avoid rebuilding, and the option sets live
 * once in `formats` in `shared/i18n/config.ts` instead of at each call site.
 *
 * So the rule is a flat ban rather than an allowlist. There is no module that
 * legitimately needs a locale-blind formatter: every one of these call sites
 * renders to a screen, and every screen has a request behind it.
 *
 * **Why a content predicate and not `dependOnFiles`.** `date-fns` and `Intl`
 * are an external package and a global; neither is a node in the project's
 * import graph, so there is nothing for a graph rule to point at. `adhereTo` is
 * the API the library documents for the remaining case, and `portable-core`
 * already leans on it for the same reason.
 */

const LOCALE_BLIND_FORMAT =
  /toLocale(?:String|DateString|TimeString)\s*\(|new Intl\.|(?:from|import)\s+['"]date-fns['"]/;

/**
 * The URL carries no locale, so nothing may route as if it did.
 *
 * **This rule used to say the opposite.** While the locale lived in the path,
 * a plain `next/link` href omitted the prefix and the proxy bounced it back as
 * a 307, so every navigation had to go through next-intl's wrapper. That whole
 * apparatus is gone: an internal dashboard sits behind a login where nothing
 * is indexed, nothing is cached per locale, and a link pasted to a colleague
 * should open in *their* language, not the sender's.
 *
 * What is left to enforce is that it does not creep back. A locale-aware
 * navigation import means someone reintroduced the wrapper; a `[locale]`
 * segment or a `proxy.ts` means someone reintroduced the routing.
 */
const LOCALE_ROUTING = /(?:from|import)\s+['"][^'"]*i18n\/navigation['"]/;

const posix = (path: string) => path.split('\\').join('/');
const isMessages = (path: string) => posix(path).includes('/messages/');

/** Every file a rule here judges: source, not tests, not the dictionaries. */
const judged = (path: string) => !isTestFile(path) && !isMessages(path);

describe('the locale reaches the data, not just the chrome', () => {
  /**
   * The floor.
   *
   * Both assertions below are negatives, and a negative over an empty set is
   * the vacuous pass AD-9 exists to prevent. archunit's own `EmptyTestViolation`
   * cannot catch it: the filter is `src/**`, which always matches, and the
   * narrowing happens inside the predicate where the library cannot see it.
   */
  it('reads the population it expects to read', async () => {
    const source = await projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo((file) => judged(file.path), 'counted');

    // 262 source files came across from `webview-ui`; the route layer
    // replaced its own.
    expect((await source.check()).length).toBeGreaterThanOrEqual(250);
  });

  /**
   * The second floor, for the navigation rule specifically.
   *
   * Forbidding a locale-aware navigation import means nothing if nobody
   * navigates. This counts the files that use the application's own navigation
   * (the router's `Link` and hooks), so a refactor that
   * empties them fails here rather than making the ban below trivially
   * satisfiable.
   */
  it('sees the navigation it claims to check', async () => {
    const users = await projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) =>
          judged(file.path) &&
          /(?:from|import)\s+['"]@tanstack\/react-router['"]/.test(
            withoutComments(file.content),
          ),
        'counted',
      );

    // 55 files navigated before the migration; the typed router kept them all.
    expect((await users.check()).length).toBeGreaterThanOrEqual(50);
  });

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

  it('routes through the router, not through a locale-aware wrapper', async () => {
    const rule = projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) =>
          judged(file.path) &&
          LOCALE_ROUTING.test(withoutComments(file.content)),
        "imports a locale-aware navigation module: the URL carries no locale, so the router's own navigation is correct here",
      );

    await expect(rule).toPassAsync();
  });

  /**
   * The routing apparatus itself, pinned by its absence.
   *
   * The rule above catches a file that *uses* locale routing. These two catch
   * someone re-adding the routing for it to use: a dynamic `[locale]` segment
   * wrapping the app, or the proxy that would have to populate it. Both are
   * cheap to check and neither has a natural reason to reappear.
   */
  it('has no locale segment in the route tree', async () => {
    const rule = projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) => posix(file.path).includes('/app/[locale]/'),
        'sits under a `[locale]` route segment: the locale comes from the reader, not the URL',
      );

    await expect(rule).toPassAsync();
  });

  it('has no proxy rewriting requests for a locale', async () => {
    const rule = projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) => /(^|\/)(proxy|middleware)\.ts$/.test(posix(file.path)),
        'is a proxy/middleware: the only one this app had existed to prefix URLs with a locale, and there is no longer a prefix',
      );

    await expect(rule).toPassAsync();
  });
});
