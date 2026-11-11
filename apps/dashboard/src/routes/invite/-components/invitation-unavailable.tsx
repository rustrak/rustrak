import { Button } from '@rustrak/ui';
import { useTranslations } from 'use-intl';
import { ErrorScreen } from '@/shared/ui/components/error-screen';
import { Link } from '@/shared/ui/components/link';

/**
 * The verdict for a token that buys nothing: never issued, malformed, expired,
 * or already spent.
 *
 * Built on `ErrorScreen`, like the 404 and the two error boundaries, because
 * this is the same kind of moment: a reader with no header, no navigation and
 * no session, meeting a dead end.
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
        <Button variant="primary" render={<Link href="/login" />}>
          {t('unavailable.goToLogin')}
        </Button>
      }
    />
  );
}
