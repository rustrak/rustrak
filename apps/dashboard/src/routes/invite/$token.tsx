import type { RustrakError } from '@rustrak/client';
import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { userQueries } from '@/features/user/api/queries';
import { translator } from '@/shared/i18n/intl';
import { OutageScreen } from '@/shared/ui/components/outage-screen';
import { RustrakWordmark } from '@/shared/ui/components/rustrak-wordmark';
import { AcceptInvitationForm } from './-components/accept-invitation-form';
import { InvitationUnavailable } from './-components/invitation-unavailable';

/**
 * The kinds that mean the token itself buys nothing.
 *
 * The server emits exactly two of them: `404` when no invitation carries the
 * token, and `400` when one does but is expired or already spent. `gone` is
 * listed because the docstring on `getInvitation` promises this page branches
 * on `kind`, and a `410` is the natural way for that endpoint to grow.
 *
 * **Written as an allowlist on purpose.** Every other kind -- an outage, a rate
 * limit from a proxy, a version mismatch -- means the question could not be
 * asked, not that the answer was no. Defaulting the unknown case to "your link
 * is dead" is the bug this replaces; defaulting it to "we could not check" is
 * wrong at worst about a cause, never about the invitation.
 */
function isDeadToken(error: RustrakError): boolean {
  return (
    error.kind === 'not_found' ||
    error.kind === 'validation' ||
    error.kind === 'gone'
  );
}

export const Route = createFileRoute('/invite/$token')({
  head: () => {
    const t = translator('invite');
    return {
      meta: [
        { title: t('meta.title') },
        { name: 'description', content: t('meta.description') },
      ],
    };
  },
  loader: ({ params, context: { queryClient } }) =>
    queryClient.ensureQueryData(userQueries.invitation(params.token)),
  component: InvitePage,
});

function InvitePage() {
  const { token } = Route.useParams();
  const { data: result } = useSuspenseQuery(userQueries.invitation(token));

  // Three outcomes, where there used to be two. `!result.success` alone folded
  // a transient outage into "this invitation link is invalid ... ask an
  // administrator to send you a new one" -- false, and the single action that
  // cannot fix an unreachable API. The invitee would burn an admin's time on a
  // link that was fine all along.
  if (!result.success && !isDeadToken(result.error)) {
    return <OutageScreen error={result.error} />;
  }

  // Belt and braces: the server's `is_acceptable` already rejects both of these
  // with a `400`, so this only fires if the two halves disagree about the
  // clock or the status.
  const invitation = result.success ? result.data : null;
  const isAcceptable =
    invitation !== null &&
    invitation.status === 'pending' &&
    new Date(invitation.expires_at).getTime() >= Date.now();

  // Both failure screens own the whole viewport, so they return before the
  // invite chrome rather than rendering inside it. What is left below is the
  // one success path.
  if (!isAcceptable) {
    return <InvitationUnavailable />;
  }

  return (
    <div className="min-h-screen bg-card flex items-center justify-center p-8 lg:p-12">
      <div className="w-full max-w-[420px] space-y-10">
        <div className="flex items-center">
          <RustrakWordmark className="h-[22px] w-auto" />
        </div>

        <AcceptInvitationForm token={token} email={invitation.email} />
      </div>
    </div>
  );
}
