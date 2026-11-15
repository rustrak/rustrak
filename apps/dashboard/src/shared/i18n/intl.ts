import type { Formats, Messages } from 'use-intl';
import type { CurrentUser, SessionStore } from '@/shared/api/session';
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
  // The newest resolve, and its number. Kept apart from `inFlight`, which a
  // failure clears, so an older resolve always has the newer one to defer to.
  let generation = 0;
  let latest: Promise<IntlSnapshot> | null = null;
  const listeners = new Set<() => void>();

  async function resolve(mine: number): Promise<IntlSnapshot> {
    const answer = await deps.readSession();
    const user = answer.state === 'authenticated' ? answer.user : null;

    const locale = resolveLocale(user?.language, deps.browserLanguages());
    const timeZone = resolveTimeZone(user?.timezone);
    const messages = await deps.loadMessages(locale);

    // A newer resolve started while this one waited: a sign-out while the
    // previous reader's catalogue was still loading. It has the newer reader,
    // so this one publishes nothing and answers with whatever that one does.
    if (mine !== generation && latest) return latest;

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
    generation += 1;
    const request = resolve(generation);
    inFlight = request;
    latest = request;
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
 * Re-resolves the configuration whenever the reader changes: a sign-in, a
 * sign-out, or a sign-in as someone else.
 *
 * The login page renders before anyone is signed in, so the snapshot holds
 * the browser's language, UTC and the language's own clock. Password sign-in,
 * an SSO link and an accepted invitation all stay inside the SPA and only
 * `session.set()` the reader, so without this their saved language, zone and
 * clock would wait for a full reload.
 *
 * Keyed on the user id, like `clearOnUserChange`. A saved preference publishes
 * the same reader again, and the control that saved it reloads on its own
 * (it waits for the reload to show its pending state). The first answer is
 * skipped: it is the one `ensure()` is resolving at boot.
 */
export function reloadOnUserChange(
  store: IntlStore,
  session: SessionStore,
): () => void {
  const reader = () => {
    const current = session.peek();
    if (!current) return undefined;
    return current.state === 'authenticated' ? current.user.id : null;
  };
  let last = reader();
  return session.subscribe(() => {
    const next = reader();
    if (next === last) return;
    const first = last === undefined;
    last = next;
    if (first) return;
    // Rejects only for a catalogue that would not load; the snapshot already
    // on screen is a better answer than an unhandled rejection.
    void store.reload().catch((error: unknown) => console.error(error));
  });
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
