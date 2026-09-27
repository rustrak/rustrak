import { Link } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { ErrorScreen } from '@/shared/ui/components/error-screen';
import { Button } from '@/shared/ui/components/shadcn/button';

/**
 * The verdict for a token that buys nothing: never issued, malformed, expired,
 * or already spent.
 *
 * Built on `ErrorScreen`, like the 404 and the two error boundaries, because
 * this is the same kind of moment: a reader with no header, no navigation and
 * no session, meeting a dead end. It used to be a small card inside the invite
 * page's own chrome, which made the one screen a signed-out person is most
 * likely to hit the only failure in the app that looked improvised.
 *
 * The brand half says links expire rather than reassuring anyone their data is
 * safe. That default belongs to an outage; here nothing broke and the reader
 * has no data yet, so the honest line is the one that explains the rule.
 *
 * It renders only for a genuinely dead token. A failed *read* -- an unreachable
 * API, a proxy rate limit -- goes to `OutageScreen` instead; see the page. That
 * distinction is the whole point, because "ask an administrator for a new link"
 * is the one action that cannot fix an API that is down.
 */
export function InvitationUnavailable() {
  const t = useTranslations('invite');

  return (
    <ErrorScreen
      brandStatement={t('unavailable.brandStatement')}
      brandDescription={t('unavailable.brandDescription')}
      headline={t('unavailable.headline')}
      description={t('unavailable.description')}
      guidance={t('unavailable.guidance')}
      actions={
        <Button nativeButton={false} render={<Link to="/login" />}>
          {t('unavailable.goToLogin')}
        </Button>
      }
    />
  );
}
