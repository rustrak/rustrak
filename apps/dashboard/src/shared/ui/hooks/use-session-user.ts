import type { UserResponse } from '@rustrak/client';
import { useRouteContext } from '@tanstack/react-router';

/**
 * The signed-in reader, for any screen under `_authenticated`.
 *
 * The gate resolved this before the route loaded and put it in the router
 * context, so there is nothing to fetch here and nothing to wait for.
 *
 * It throws rather than returning a nullable user, because the layout above
 * renders the outage screen instead of `<Outlet />` when there is no session:
 * a component reaching this line has already been guaranteed one, and a
 * nullable type would make every call site handle a branch that cannot happen.
 */
export function useSessionUser(): UserResponse {
  const context = useRouteContext({ from: '/_authenticated' });

  if (context.state !== 'authenticated') {
    throw new Error(
      'useSessionUser was called outside `_authenticated`, where there is no ' +
        'guaranteed session.',
    );
  }

  return context.user;
}
