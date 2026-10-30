import type { QueryClient } from '@tanstack/react-query';
import { createRootRouteWithContext, Outlet } from '@tanstack/react-router';

/**
 * Everything above routing.
 *
 * Deliberately thin. The providers a Next root layout carried — the theme, the
 * toaster and the message catalogue — are mounted in `main.tsx` instead,
 * *outside* the router, because none of them may be torn down and rebuilt by a
 * navigation. A theme provider under the router would re-read `localStorage`
 * on a route change and a toast raised by a mutation would be unmounted by the
 * navigation that mutation triggered.
 *
 * No `<HeadContent />` and no route `head`: a page titles the tab with its
 * own `<title>`, which React hoists into the document and keeps current as
 * its data changes. A `head` could only read loader data, and loaders here
 * return none.
 *
 * The context carries the one `QueryClient`, so every loader reads and fills
 * the same cache the components subscribe to.
 */
export const Route = createRootRouteWithContext<{
  queryClient: QueryClient;
}>()({
  component: RootLayout,
});

function RootLayout() {
  return <Outlet />;
}
