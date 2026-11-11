import type { QueryClient } from '@tanstack/react-query';
import { createRootRouteWithContext } from '@tanstack/react-router';

/**
 * Everything above routing. The theme, the toaster and the message catalogue
 * are mounted in `main.tsx`, *outside* the router, so no navigation tears them
 * down: a toast raised by a mutation would otherwise be unmounted by the
 * navigation that mutation triggered.
 *
 * The context carries the one `QueryClient`, so every loader reads and fills
 * the same cache the components subscribe to.
 */
export const Route = createRootRouteWithContext<{
  queryClient: QueryClient;
}>()({});
