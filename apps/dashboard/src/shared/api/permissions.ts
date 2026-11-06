import { noop, type QueryClient, queryOptions } from '@tanstack/react-query';
import { type Permissions, permissionsFrom } from '@/shared/lib/permissions';
import { orThrow } from './query';
import { api } from './rustrak';

/**
 * What the reader may do. In `shared` for the same reason as the session:
 * the frame, the settings pages and several slices all gate on it.
 */
export const accessQueries = {
  root: () => ['access'] as const,
  me: () =>
    queryOptions({
      queryKey: [...accessQueries.root(), 'me'],
      queryFn: ({ signal }) => orThrow(api().access.me({ signal })),
    }),
  /** Every resource a permission can name. Fixed for a server's lifetime. */
  catalog: () =>
    queryOptions({
      queryKey: [...accessQueries.root(), 'catalog'],
      queryFn: ({ signal }) => orThrow(api().access.catalog({ signal })),
      staleTime: Number.POSITIVE_INFINITY,
    }),
};

/**
 * For a loader: start `start`'s reads once the reader is known to be
 * `allowed`, without making the router wait. Reads a non-permitted reader
 * would only get 403s for are never made.
 */
export function startIfAllowed(
  queryClient: QueryClient,
  allowed: (permissions: Permissions) => boolean,
  start: () => void,
): void {
  void queryClient
    .query(accessQueries.me())
    .then((me) => {
      if (allowed(permissionsFrom(me))) start();
    })
    .catch(noop);
}
