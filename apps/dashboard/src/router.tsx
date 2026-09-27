import type { RustrakError } from '@rustrak/client';
import { createRouter } from '@tanstack/react-router';
import { bindCacheToSession, queryClient } from '@/shared/api/query-client';
import { session } from '@/shared/api/session';
import { AppErrorScreen } from '@/shared/ui/components/app-error-screen';
import { NotFoundScreen } from '@/shared/ui/components/not-found-screen';
import { routeTree } from './routeTree.gen';

/**
 * The router.
 *
 * `basepath` is left at `/` deliberately: the bundle is served from the root of
 * the server's origin, and a sub-path deployment would have to change this
 * *and* `base` in `vite.config.ts` together.
 */
export function createAppRouter() {
  return createRouter({
    routeTree,
    context: { queryClient },
    // Every screen fetches through a loader, and the loaders are the reason
    // there is no spinner between two pages of the same table: the router
    // keeps the old page rendered until the new data is in. `0` would flash
    // the pending component on a fast network, which is worse than waiting.
    defaultPendingMs: 300,
    defaultPendingMinMs: 300,
    defaultPreload: 'intent',
    // The query cache decides what is fresh. A router cache in front of it
    // would skip the loader, and with it the cache's own staleness check.
    defaultPreloadStaleTime: 0,
    // Loader data is plain JSON, so a re-run that changed nothing keeps the
    // same references and a `select` sees no change.
    defaultStructuralSharing: true,
    defaultNotFoundComponent: NotFoundScreen,
    defaultErrorComponent: AppErrorScreen,
    // Scroll restoration is what a browser does for a document and what a
    // client-routed application has to do for itself. Without it, following a
    // link from halfway down the issues list opens the issue halfway down.
    scrollRestoration: true,
  });
}

/** A read that came back `unauthenticated`: the session ended under us. */
function isExpired(data: unknown): boolean {
  const result = data as { success?: boolean; error?: RustrakError } | null;
  return result?.success === false && result.error?.kind === 'unauthenticated';
}

/**
 * What the router needs from the rest of the application, once.
 *
 * A session can end between the guard and a read. Every read goes through the
 * cache, so the cache is where that is noticed: the session is cleared first,
 * so the guard on the way back in does not wave the visitor through on the
 * answer this response just disproved, and they return here after signing in.
 */
export function connectRouter(router: ReturnType<typeof createAppRouter>) {
  bindCacheToSession(queryClient, session);

  queryClient.getQueryCache().subscribe((event) => {
    if (event.type !== 'updated' || event.action.type !== 'success') return;
    if (!isExpired(event.action.data)) return;
    if (session.peek()?.state === 'anonymous') return;

    session.clear();
    void router.navigate({
      to: '/login',
      search: { redirect: router.state.location.href },
    });
  });
}

declare module '@tanstack/react-router' {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}
