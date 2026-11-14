import type { Formats, Messages } from 'use-intl';
import type { CurrentUser } from '@/shared/api/session';
import { session } from '@/shared/api/session';
import type { Locale } from './config';
import {
  formatsFor,
  resolveHourCycle,
  resolveLocale,
  resolveTimeZone,
} from './config';

/**
 * The i18n configuration, resolved once for the reader at boot from
 * `users.language`, `users.timezone` and `users.clock_24h`, and republished
 * when the reader changes any of them.
 */
export interface IntlSnapshot {
  locale: Locale;
  timeZone: string;
  messages: Messages;
  formats: Formats;
  // No `now`: it moves after every read, and here it would re-render every
  // `useTranslations`. It is `clock.ts`.
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
  /** Re-resolve, after the reader changed their language, zone or clock. */
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
    const answer = await deps.readSession();
    const user = answer.state === 'authenticated' ? answer.user : null;

    const locale = resolveLocale(user?.language, deps.browserLanguages());
    const timeZone = resolveTimeZone(user?.timezone);
    const messages = await deps.loadMessages(locale);

    current = {
      locale,
      timeZone,
      messages,
      formats: formatsFor(
        timeZone,
        resolveHourCycle(user?.clock_24h),
      ) as Formats,
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
 * `navigator.languages` is the browser's `Accept-Language` list. Guarded
 * because a browser that exposes only `navigator.language` is still a browser.
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
