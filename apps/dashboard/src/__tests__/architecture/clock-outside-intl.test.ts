import { projectFiles } from 'archunit';
import { describe, expect, it } from 'vitest';
import { isTestFile, posix, withoutComments } from './predicates';

/**
 * Relative times read `now` from `shared/i18n/clock.ts`, never from use-intl.
 *
 * `now` moves after every read that succeeds. It used to be a prop of
 * `IntlProvider`, and a new prop there is a new context value for every
 * `useTranslations` and `useFormatter` in the application: measured on the
 * issues list, each read that landed anywhere re-rendered ~1,860 components
 * and held the main thread ~35 ms. Out of the provider, a move re-renders
 * only what reads the clock (`useNow`, `useRelativeTime`).
 *
 * The provider has no `now` any more, so use-intl's own `useNow()` and a bare
 * `format.relativeTime(date)` would read the browser's clock instead: each
 * row of a list against its own instant, and a time fetched a moment ago
 * could read "in 2 seconds".
 */

/** The one module that calls use-intl's `relativeTime`, with the clock's `now`. */
const isTheHook = (path: string) =>
  posix(path).endsWith('src/shared/i18n/use-now.ts');

const BARE_RELATIVE_TIME = /\.relativeTime\(/;
const USE_INTL_NOW = /import\s*{[^}]*\buseNow\b[^}]*}\s*from\s*'use-intl'/;

describe('relative times read the clock, not the intl provider', () => {
  it('reads the population it expects to read', async () => {
    const readers = await projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) =>
          !isTestFile(file.path) &&
          /\b(useRelativeTime|useNow)\(/.test(withoutComments(file.content)),
        'counted',
      )
      .check();

    // The issues list, the monitors and every "x minutes ago" on a page.
    expect(readers.length).toBeGreaterThanOrEqual(20);
  });

  it('has no relativeTime call outside useRelativeTime', async () => {
    const rule = projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) =>
          !isTestFile(file.path) &&
          !isTheHook(file.path) &&
          BARE_RELATIVE_TIME.test(withoutComments(file.content)),
        'formats a relative time against a now that is not the clock',
      );

    await expect(rule).toPassAsync();
  });

  it('has no use-intl useNow, whose now the provider no longer carries', async () => {
    const rule = projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) =>
          !isTestFile(file.path) &&
          USE_INTL_NOW.test(withoutComments(file.content)),
        'imports useNow from use-intl',
      );

    await expect(rule).toPassAsync();
  });
});
