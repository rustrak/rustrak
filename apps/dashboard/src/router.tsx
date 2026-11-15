import type { QueryClient } from '@tanstack/react-query';
import { createRouter } from '@tanstack/react-router';
import { pageTransition } from '@/shared/lib/page-transition';
import { NotFoundScreen } from '@/shared/ui/components/not-found-screen';
import { RouteError } from '@/shared/ui/components/route-error';
import { routeTree } from './routeTree.gen';

/** Below `md`, where the page slides like an app's, unless asked not to. */
const animatesPages = () =>
  window.matchMedia('(width < 48rem)').matches &&
  !window.matchMedia('(prefers-reduced-motion: reduce)').matches;

/**
 * The router.
 *
 * `basepath` is left at `/` deliberately: the bundle is served from the root of
 * the server's origin, and a sub-path deployment would have to change this
 * *and* `base` in `vite.config.ts` together.
 */
export function createAppRouter(queryClient: QueryClient) {
  return createRouter({
    routeTree,
    context: { queryClient },
    // No pending timing here: loaders never wait, so a navigation commits at
    // once and the page's own `pendingComponent` is what shows while its
    // reads are in flight (see "Where the data comes from" in CLAUDE.md).
    // Hovering a link starts its reads. Whether a read is fresh enough to
    // skip is the query cache's decision, not the router's: with the router's
    // own 30s preload cache on top, a preload could hold a loader result that
    // the cache has since invalidated.
    defaultPreload: 'intent',
    defaultPreloadStaleTime: 0,
    defaultNotFoundComponent: NotFoundScreen,
    defaultErrorComponent: RouteError,
    // Scroll restoration is what a browser does for a document and what a
    // client-routed application has to do for itself. Without it, following a
    // link from halfway down the issues list opens the issue halfway down.
    scrollRestoration: true,
    // On a phone a navigation moves the page the way an app's would: deeper
    // slides in from the right, back slides in from the left, a tab
    // cross-fades (`shared/lib/page-transition.ts`; the motion is in
    // `styles.css`). A desktop navigates in place, as it always has.
    defaultViewTransition: {
      types: ({ fromLocation, toLocation }) =>
        animatesPages() ? pageTransition(fromLocation, toLocation) : false,
    },
  });
}

declare module '@tanstack/react-router' {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}
