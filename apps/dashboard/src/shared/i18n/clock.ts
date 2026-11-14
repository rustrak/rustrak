/**
 * One shared "now" for every relative time on screen, moved forward each time
 * a read succeeds (`refreshNowOnData` in `shared/api/query.ts`).
 *
 * Without it each `relativeTime` call reaches for its own `new Date()`: two
 * rows rendered microseconds apart get measured against two different clocks,
 * so a list of timestamps can disagree with itself at a boundary. Moving it on
 * each read keeps data fetched later from reading against an older clock ("in
 * 3 minutes"). Between reads it stays put, so relative times do not tick on
 * their own: the right default for rows that are minutes to weeks old.
 *
 * Its own store, not a prop of `IntlProvider`: a new provider value
 * re-renders every `useTranslations` in the application, and this moves after
 * every read. Read it with `useNow` and `useRelativeTime` (`use-now.ts`).
 *
 * Never build a query key from it: each success moves it, and the new key
 * would fetch again.
 */
export interface Clock {
  now(): Date;
  /** Move `now` to the present and tell whoever reads it. */
  refreshNow(): void;
  subscribe(listener: () => void): () => void;
}

export function createClock(): Clock {
  let current = new Date();
  const listeners = new Set<() => void>();
  return {
    now: () => current,
    refreshNow() {
      current = new Date();
      for (const listener of listeners) listener();
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

export const clock = createClock();
