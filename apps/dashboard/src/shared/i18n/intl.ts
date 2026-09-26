import type { Formats, Messages } from 'use-intl';
import { createTranslator } from 'use-intl';
import type { CurrentUser } from '@/shared/api/session';
import { session } from '@/shared/api/session';
import { formatsFor, resolveLocale, resolveTimeZone } from './config';
import type { Locale } from './routing';

/**
 * The i18n configuration, resolved once for the reader rather than once per
 * request.
 *
 * Under Next this was `getRequestConfig`: the server knew who was asking, read
 * `users.language` and `users.timezone` off the row, and handed the whole
 * thing to `NextIntlClientProvider`. A static bundle has no request to hang
 * that on, so the same three decisions are made once at boot, from the same
 * two columns, and republished when the reader changes either.
 */
export interface IntlSnapshot {
  locale: Locale;
  timeZone: string;
  messages: Messages;
  formats: Formats;
  /**
   * One "now" for the whole session.
   *
   * Without it every `format.relativeTime` call reaches for its own
   * `new Date()`, which next-intl warned about (`ENVIRONMENT_FALLBACK`) for a
   * reason that still bites: two rows rendered microseconds apart get measured
   * against two different clocks, so a list of timestamps can disagree with
   * itself at a boundary.
   *
   * Relative times therefore do not tick on their own. That is the right
   * default for a dashboard whose rows are minutes to weeks old; a surface
   * that genuinely needs a live clock asks for one with
   * `useNow({updateInterval})`.
   */
  now: Date;
}

/** The three things the store needs, named so tests can stub them. */
export interface IntlDependencies {
  readSession(): Promise<CurrentUser>;
  /** The reader's languages, in preference order. `navigator.languages`. */
  browserLanguages(): readonly string[];
  loadMessages(locale: Locale): Promise<Messages>;
}

export interface IntlStore {
  /** Resolve the configuration. Idempotent: the second call is the first. */
  ensure(): Promise<IntlSnapshot>;
  /** Re-resolve, after the reader changed their language or their zone. */
  reload(): Promise<IntlSnapshot>;
  /** The resolved configuration. Throws before `ensure()` has settled. */
  snapshot(): IntlSnapshot;
  subscribe(listener: () => void): () => void;
}

export function createIntlStore(deps: IntlDependencies): IntlStore {
  let current: IntlSnapshot | undefined;
  let inFlight: Promise<IntlSnapshot> | null = null;
  const listeners = new Set<() => void>();

  async function resolve(): Promise<IntlSnapshot> {
    // The browser's language is the answer for most readers, so its
    // catalogue loads alongside the session instead of after it.
    const guess = resolveLocale(undefined, deps.browserLanguages());
    const guessed = deps.loadMessages(guess);
    guessed.catch(() => undefined);

    const answer = await deps.readSession();
    const user = answer.state === 'authenticated' ? answer.user : null;

    const locale = resolveLocale(user?.language, deps.browserLanguages());
    const timeZone = resolveTimeZone(user?.timezone);
    const messages =
      locale === guess ? await guessed : await deps.loadMessages(locale);

    current = {
      locale,
      timeZone,
      messages,
      formats: formatsFor(timeZone) as Formats,
      now: new Date(),
    };
    for (const listener of listeners) listener();
    return current;
  }

  function start(): Promise<IntlSnapshot> {
    const request = resolve();
    inFlight = request;
    // A failed resolve must not be remembered: it is almost always the
    // session read, and the next navigation gets to try again.
    void request.catch(() => {
      if (inFlight === request) inFlight = null;
    });
    return request;
  }

  return {
    ensure() {
      return inFlight ?? start();
    },
    reload() {
      return start();
    },
    snapshot() {
      if (!current) {
        throw new Error(
          'The messages have not been resolved yet. `intl.ensure()` runs in ' +
            'main.tsx before the first render for exactly this reason.',
        );
      }
      return current;
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

/**
 * The catalogues, one dynamic import each.
 *
 * Five languages, and a reader uses one. Naming them in a literal rather than
 * interpolating the locale into the specifier is what lets the bundler see
 * five chunks instead of shipping all five to everyone.
 */
const CATALOGUES: Record<Locale, () => Promise<{ default: Messages }>> = {
  en: () => import('./messages/en.json'),
  es: () => import('./messages/es.json'),
  fr: () => import('./messages/fr.json'),
  ro: () => import('./messages/ro.json'),
  zh: () => import('./messages/zh.json'),
};

async function loadMessages(locale: Locale): Promise<Messages> {
  return (await CATALOGUES[locale]()).default;
}

/**
 * The reader's languages, in preference order.
 *
 * `navigator.languages` is the same list the browser used to send as
 * `Accept-Language`, which is what the Next version read. Guarded because a
 * browser that exposes only `navigator.language` is still a browser.
 */
function browserLanguages(): readonly string[] {
  if (typeof navigator === 'undefined') return [];
  return navigator.languages?.length
    ? navigator.languages
    : [navigator.language].filter(Boolean);
}

export const intl = createIntlStore({
  readSession: () => session.ensure(),
  browserLanguages,
  loadMessages,
});

/**
 * A translator outside React.
 *
 * Route titles are the whole reason this exists: TanStack resolves `head`
 * before a component tree exists, so it cannot call `useTranslations`. It
 * reads the same snapshot the provider renders from, so a title and the page
 * under it can never be in different languages.
 */
export function translator(namespace?: string) {
  const { locale, messages, timeZone, formats, now } = intl.snapshot();
  return createTranslator({
    locale,
    messages,
    timeZone,
    formats,
    now,
    namespace,
  });
}
