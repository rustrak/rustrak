import type { RustrakError } from '@rustrak/client';
import { NotFoundScreen } from '@/shared/ui/components/not-found-screen';
import { ServiceUnavailable } from '@/shared/ui/components/service-unavailable';

/**
 * What a screen renders when a `Result` it needed came back a failure.
 *
 * Three outcomes, and which one you get is decided by `kind` alone:
 *
 * - `unauthenticated` renders nothing. The guard already gated the route, so
 *   it means the session expired since; `router.tsx` watches every read for
 *   that answer and sends the visitor to sign in, once, from outside React.
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
  if (error.kind === 'unauthenticated') return null;

  if (error.kind === 'not_found' && notFoundOnMissing) {
    return <NotFoundScreen />;
  }

  return <ServiceUnavailable error={error} title={title} />;
}
