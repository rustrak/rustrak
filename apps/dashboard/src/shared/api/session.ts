import type { RustrakError, UserResponse } from '@rustrak/client';
import { api } from './rustrak';

/**
 * Whether there is a session, and if not, why not.
 *
 * Three states, not a nullable user. `anonymous` is the *only* one that may
 * send the visitor to `/login`: an unreachable API or a 5xx is
 * `unavailable`, and redirecting on those turns a flaky connection into a
 * login loop that logging in cannot fix, because the next request fails the
 * same way. A 403 is `unavailable` too, since it means "signed in, not
 * allowed", which login also does not fix.
 */
export type CurrentUser =
  | { state: 'authenticated'; user: UserResponse }
  | { state: 'anonymous' }
  | { state: 'unavailable'; error: RustrakError };

/**
 * Ask the server who is asking.
 *
 * **It lives in `shared`, not in `features/user`, and that is the rule rather
 * than an exception to it.** Three unrelated things need this answer before
 * anything renders: the router's guard, `shared/i18n` (the reader's language
 * and zone are columns on the user row), and the account screens. A type two
 * consumers both need goes *down* into `shared`, never sideways — and
 * `shared/api` already owns client construction, so the read belongs beside
 * it.
 */
export async function getCurrentUser(): Promise<CurrentUser> {
  const result = await api().auth.getCurrentUser();

  if (result.success) {
    return { state: 'authenticated', user: result.data };
  }

  if (result.error.kind === 'unauthenticated') {
    return { state: 'anonymous' };
  }

  return { state: 'unavailable', error: result.error };
}

/** The session, as the router and the account screens read it. */
export interface SessionStore {
  /** Memoises the in-flight request, so nested guards share one `/auth/me`. */
  ensure(): Promise<CurrentUser>;
  /** The settled session, or `undefined` before `ensure()` resolves. */
  peek(): CurrentUser | undefined;
  /** Adopt a session a sign-in just proved, with no second round trip. */
  set(session: CurrentUser): void;
  /** Drop the session this tab holds. The next guard asks for the truth. */
  clear(): void;
  subscribe(listener: () => void): () => void;
}

/**
 * The store, over any reader.
 *
 * Nothing in the SPA re-renders on the server, so "who is asking" is answered
 * once and held. The two things that has to get right:
 *
 * - **One request, not one per guard.** `_authenticated` and every layout
 *   under it want the same answer at the same moment, and without the memo
 *   each nested guard issues its own `/auth/me`.
 * - **`unavailable` is not remembered.** It is the one answer a retry can
 *   change; caching it makes a momentary drop permanent for the life of the
 *   tab, on every route.
 */
export function createSessionStore(
  read: () => Promise<CurrentUser>,
): SessionStore {
  let inFlight: Promise<CurrentUser> | null = null;
  let settled: CurrentUser | undefined;
  const listeners = new Set<() => void>();

  function publish(session: CurrentUser): CurrentUser {
    settled = session;
    for (const listener of listeners) listener();
    return session;
  }

  return {
    ensure() {
      if (inFlight) return inFlight;

      const request = read().then(publish);
      inFlight = request;

      const forget = () => {
        if (inFlight === request) inFlight = null;
      };
      // A rejection is forgotten for the same reason `unavailable` is: it is
      // a failure to *ask*, not an answer, and a memoised rejected promise
      // means the tab never recovers.
      void request.then(
        (session) => session.state === 'unavailable' && forget(),
        forget,
      );

      return request;
    },

    peek() {
      return settled;
    },

    set(session) {
      inFlight = Promise.resolve(session);
      publish(session);
    },

    clear() {
      inFlight = null;
      publish({ state: 'anonymous' });
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
 * The application's session.
 *
 * A module singleton rather than router context: `shared/i18n` needs it during
 * bootstrap, before a router exists, and a second store would mean a second
 * `/auth/me` and two answers that can disagree.
 */
export const session = createSessionStore(getCurrentUser);
