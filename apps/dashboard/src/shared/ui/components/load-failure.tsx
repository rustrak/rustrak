import type { RustrakError } from '@rustrak/client';
import { useEffect } from 'react';
import { session } from '@/shared/api/session';
import { NotFoundScreen } from '@/shared/ui/components/not-found-screen';
import { ServiceUnavailable } from '@/shared/ui/components/service-unavailable';
import { useRouter } from '@/shared/ui/hooks/use-router';

/**
 * What a screen renders when a `Result` it needed came back a failure.
 *
 * Three outcomes, and which one you get is decided by `kind` alone:
 *
 * - `unauthenticated` is the *only* kind that sends the visitor to login. The
 *   `_authenticated` guard already gated the route, so reaching this means the
 *   session expired between the guard and the fetch.
 * - `not_found` renders the app's 404. A project-scoped endpoint answering 404
 *   means the project, issue or event in the URL is gone, which is a wrong
 *   address rather than an outage.
 * - everything else renders an outage surface, in place, with no navigation.
 *   An unreachable API must never look like a missing record or a signed-out
 *   session, because neither of those is something the user can act on.
 *
 * `title` says which fetch failed, so a page that loads several things does not
 * leave the reader guessing which one is missing.
 *
 * **The two navigating branches are in the render.** A router redirect thrown
 * from a component is not caught before paint: the tree is already
 * rendering. So the sign-out case navigates from an effect and paints
 * nothing in the meantime, and the missing-record case renders the same 404
 * screen the route-level handler renders rather than routing to it — the
 * reader sees the same thing either way, and the address stays on the record
 * they asked for rather than being rewritten to a 404 URL they cannot share.
 */
export function LoadFailure({
  error,
  title,
  notFoundOnMissing = true,
}: {
  error: RustrakError;
  title: string;
  /**
   * Set `false` where a 404 is not "the thing in the URL is gone" but "this
   * particular endpoint had nothing", so the surrounding page survives it.
   */
  notFoundOnMissing?: boolean;
}) {
  const router = useRouter();
  const expired = error.kind === 'unauthenticated';

  useEffect(() => {
    if (!expired) return;
    // Cleared first: the guard on the way back in must not read the session
    // this very response just disproved and wave the visitor through.
    session.clear();
    router.replace('/login');
  }, [expired, router]);

  if (expired) return null;

  if (error.kind === 'not_found' && notFoundOnMissing) {
    return <NotFoundScreen />;
  }

  return <ServiceUnavailable error={error} title={title} />;
}
