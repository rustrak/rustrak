import { useNavigate, useRouterState } from '@tanstack/react-router';
import { useMemo } from 'react';

/** Navigation by `href`, for the call sites that build one. */
export function useRouter() {
  const navigate = useNavigate();

  return useMemo(
    () => ({
      push: (href: string) => {
        void navigate({ href, state: { entry: true } });
      },
      replace: (href: string) => {
        void navigate({ href, replace: true });
      },
    }),
    [navigate],
  );
}

declare module '@tanstack/react-router' {
  interface HistoryState {
    /** Arrived by a link or `push`, not by the page changing its own search. */
    entry?: boolean;
  }
}

/**
 * Whether the reader came in through a link (the logo, the sidebar, a
 * `push`), as opposed to the page rewriting its own address: a filter
 * cleared on the page and a visit by its link can be the same address.
 *
 * Read from the location on screen (`resolvedLocation`), not the one being
 * loaded: a page on its way out must not take the next page's link for its
 * own arrival.
 */
export function useEntered(): boolean {
  return useRouterState({
    select: (state) => state.resolvedLocation?.state.entry === true,
  });
}
