import { projectFiles } from 'archunit';
import { describe, expect, it } from 'vitest';
import en from '../../shared/i18n/messages/en.json';
import es from '../../shared/i18n/messages/es.json';
import fr from '../../shared/i18n/messages/fr.json';
import ro from '../../shared/i18n/messages/ro.json';
import zh from '../../shared/i18n/messages/zh.json';
import { isMessages, isTestFile, withoutComments } from './predicates';

/**
 * The message dictionaries can drift apart in three ways, and each one
 * rendered a `MISSING_MESSAGE` error in the browser before this suite
 * existed:
 *
 *  1. a key used in code that was never added to the JSON at all
 *     (`commands.close` was called by the command-bar footer and missing
 *     from both locales),
 *  2. a key added to `en` but not `zh` (use-intl renders the key name),
 *  3. a key nested under the wrong namespace, so `useTranslations('x')`
 *     plus `t('y.z')` resolves `x.y.z` which does not exist (`roles.*`
 *     was stored at the top level while the header resolved it through the
 *     `user` namespace).
 *
 * The suite below checks all three statically, over the real source files.
 * The namespace resolution is deliberately simple: a file's hook decides
 * the prefix, and a `t()` call that carries its own full path (global
 * `useTranslations()` or a model-emitted `labelKey`) is resolved as-is.
 * That matches how the code actually calls the translator.
 */

type Messages = Record<string, unknown>;

function leafKeys(obj: Messages, prefix = ''): string[] {
  const keys: string[] = [];
  for (const [name, value] of Object.entries(obj)) {
    const path = prefix ? `${prefix}.${name}` : name;
    if (typeof value === 'string') {
      keys.push(path);
    } else {
      keys.push(...leafKeys(value as Messages, path));
    }
  }
  return keys;
}

function hasKey(obj: Messages, path: string): boolean {
  let current: unknown = obj;
  for (const segment of path.split('.')) {
    if (
      typeof current !== 'object' ||
      current === null ||
      !(segment in current)
    ) {
      return false;
    }
    current = (current as Messages)[segment];
  }
  return typeof current === 'string';
}

/**
 * Every translator a file binds, with the namespace it was bound to.
 *
 * `useTranslations('ns')`; one bound with no argument is global, and its
 * namespace is the empty string, which makes each of its keys a full path.
 */
const TRANSLATOR_BINDING =
  /(?:const|let)\s+([A-Za-z_$][\w$]*)\s*=\s*useTranslations\(\s*(?:'([a-zA-Z]+)')?\s*\)/g;

/** A call on a named translator: `typesT('newIssue.name')`, `t.rich('x')`. */
const callsOf = (binding: string) =>
  new RegExp(
    `(?<![A-Za-z0-9_$])${binding}(?:\\.rich)?\\('([a-zA-Z][a-zA-Z0-9.]*)'`,
    'g',
  );

/**
 * A message key mapped to a sentence, which is what a second dictionary looks
 * like: `'error.headline.network': 'Rustrak is not responding'`. The key has
 * at least one dot; the value must be a string literal (so a ternary's colon
 * is not one) that looks like a sentence (more than one word, or ending in
 * punctuation), so a field map such as `'credentials.url': 'url'` is not.
 */
const DICTIONARY_ENTRY =
  /'[a-z][a-zA-Z]*(?:\.[a-zA-Z]+)+'\s*:\s*\n?\s*'[^']*(?:\s[^']+|[.!?])'/;

/**
 * Every locale held to the same rule as `en`. Adding a locale means adding it
 * here; a dictionary that drifts from `en` fails the build, so a missing key
 * can never reach use-intl and render as its own key name.
 */
const OTHER_LOCALES: readonly (readonly [string, Messages])[] = [
  ['zh', zh],
  ['fr', fr],
  ['es', es],
  ['ro', ro],
];

/** A file whose translator calls this rule can judge. */
function isJudgeable(path: string): boolean {
  if (isTestFile(path)) return false;
  // The dictionaries themselves hold the keys; they do not call them.
  return !isMessages(path);
}

/**
 * Every fully-qualified message key this file asks for.
 *
 * A generator so the two nested walks -- translators, then the calls on each
 * -- stay one flat statement at the call site rather than a loop inside a
 * loop inside a predicate.
 */
function* translatedKeys(content: string): Generator<string> {
  for (const binding of content.matchAll(TRANSLATOR_BINDING)) {
    const [, name, namespace = ''] = binding;

    for (const call of content.matchAll(callsOf(name))) {
      const key = call[1];
      yield namespace ? `${namespace}.${key}` : key;
    }
  }
}

describe('message dictionaries stay resolvable', () => {
  it('every locale exposes exactly the same keys as en', () => {
    const enKeys = leafKeys(en as Messages);
    for (const [locale, dictionary] of OTHER_LOCALES) {
      const keys = leafKeys(dictionary);
      const missing = enKeys.filter((key) => !keys.includes(key));
      const extra = keys.filter((key) => !enKeys.includes(key));

      expect(missing, `${locale} is missing keys`).toEqual([]);
      expect(extra, `${locale} has extra keys`).toEqual([]);
    }
    // A floor, not just an empty comparison: 1000+ keys across five locales
    // after the i18n phase, so a glob that stopped matching would fail here
    // rather than pass silently.
    expect(enKeys.length).toBeGreaterThanOrEqual(1000);
  });

  /**
   * No key may contain a dot, because `t()` reads one as a path separator.
   *
   * `"issue.title": "The error title"` looks like a key and is not one: it is
   * a *flat* entry whose name happens to hold a dot, so `t('variables.issue.
   * title')` walks `variables -> issue -> title`, finds no `issue` object, and
   * renders the fallback. The custom webhook template editor shipped ten of
   * them — every `issue.*` and `project.*` description in its variable palette
   * — and the reader saw the fallback where the explanation should be.
   *
   * Neither rule below could see it. They check the keys a file *asks for*,
   * and this editor asks with a template literal:
   * ``t(`variables.${variable.descriptionKey}`)``. Nothing static can resolve
   * that, which is exactly why the dictionary has to be checkable on its own.
   *
   * The check is the gap between this file's two helpers. `leafKeys` builds a
   * path by joining names with a dot, so a dotted name is indistinguishable
   * from nesting and it reports something that looks resolvable. `hasKey`
   * splits on the dot and walks, which is what use-intl does. Every path the
   * first produces, the second must be able to reach.
   */
  it('has no message key that use-intl cannot address', () => {
    const unreachable = leafKeys(en as Messages).filter(
      (key) => !hasKey(en as Messages, key),
    );

    expect(
      unreachable,
      'these keys contain a literal dot, so t() cannot resolve them; nest them instead',
    ).toEqual([]);
  });

  /**
   * **Per translator, not per file.**
   *
   * The first version of this resolved one namespace for the whole file -- the
   * first `useTranslations('x')` it found -- and only looked at calls on a
   * translator literally named `t`. Both halves of that were wrong, and they
   * hid a live bug between them: `alert-rules-table.tsx` bound a second
   * translator, `const typesT = useTranslations('alertTypes')`, to a namespace
   * that has never existed in `en.json`. The keys it resolved live under
   * `alerts`. Every alert rule's trigger column rendered
   * `alertTypes.newIssue.name`.
   *
   * The rule below binds each translator to its own namespace and checks the
   * calls made on that binding, so a file may hold as many as it likes and an
   * aliased one is no longer invisible.
   */
  it('every translator call resolves to an existing message key', async () => {
    const rule = projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) =>
          isJudgeable(file.path) &&
          [...translatedKeys(file.content)].some(
            (key) => !hasKey(en as Messages, key),
          ),
        'calls a translator with a message key that does not exist in en.json',
      );

    await expect(rule).toPassAsync();
  });

  /**
   * The namespace itself has to exist.
   *
   * The rule above can only judge a call whose key is a string literal, and the
   * codebase deliberately has calls whose key is not: `commands.ts` emits
   * `labelKey`, `alert-types.ts` emits `nameKey`, and the component resolves
   * whatever it is handed. That is a good pattern -- it keeps the static tables
   * free of copy -- and it means the key is unavailable to a content rule.
   *
   * What is always a literal is the namespace. `alert-rules-table.tsx` bound
   * `useTranslations('alertTypes')` and fed it `nameKey` values, so every
   * trigger cell rendered `alertTypes.newIssue.name`: the keys were real, the
   * namespace was not, and no assertion in this file could see either half.
   * Checking the namespace catches the whole class, cheaply.
   */
  it('binds every translator to a namespace that exists', async () => {
    const rule = projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo((file) => {
        if (isTestFile(file.path)) return false;

        for (const binding of file.content.matchAll(TRANSLATOR_BINDING)) {
          const namespace = binding[2];
          // A global translator resolves full paths and names no namespace.
          if (!namespace) continue;
          if (!(namespace in (en as Messages))) return true;
        }
        return false;
      }, 'binds a translator to a namespace that does not exist in en.json');

    await expect(rule).toPassAsync();
  });

  /**
   * The floor for the rule above.
   *
   * It is a negative over whatever `TRANSLATOR_BINDING` happens to match, so a
   * regex that stops matching turns it into a rule that checks nothing while
   * still reporting success. This counts the files it found at least one
   * translator in.
   */
  it('finds the translators it claims to check', async () => {
    const withTranslator = await projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) =>
          !isTestFile(file.path) &&
          // `matchAll` over a fresh iterator rather than `.test`, which would
          // advance `lastIndex` on the shared /g regex and make every other
          // file read as having no translator at all.
          [...file.content.matchAll(TRANSLATOR_BINDING)].length > 0,
        'counted',
      )
      .check();

    // 150 files bind a translator after the i18n pass.
    expect(withTranslator.length).toBeGreaterThanOrEqual(140);
  });

  /**
   * There is one dictionary, and it is `messages/`.
   *
   * `error-copy.ts` shipped with its own copy of 21 English sentences, keyed by
   * the same message keys, behind an optional translator parameter: pass `t`
   * and you get the JSON, omit it and you get the copy baked into the module.
   * All eleven call sites pass `t`, so the second table was unreachable, and
   * the two were only in sync because nothing had edited either yet. The test
   * above could not see the drift: it looks for `t('key')`, and those keys were
   * read through a `text(t, 'key')` helper instead.
   *
   * The fix a rule like this forces is the right one anyway. A module in the
   * portable core should not hold English; it should name keys and let the
   * caller resolve them, which is what `commands.ts` already does with
   * `labelKey`.
   */
  it('keeps the English copy in one place', async () => {
    const rule = projectFiles()
      .inFolder('src/**')
      .shouldNot()
      .adhereTo(
        (file) =>
          !isTestFile(file.path) &&
          !isMessages(file.path) &&
          DICTIONARY_ENTRY.test(withoutComments(file.content)),
        'holds a second copy of the message dictionary: the sentences belong in messages/*.json, and this module should name keys instead',
      );

    await expect(rule).toPassAsync();
  });
});
