import { Button, RefreshIcon } from '@rustrak/ui';
import { useTranslations } from 'use-intl';

/**
 * A reload button for a screen rendered by a Server Component.
 *
 * `router.refresh()` is deliberately not used: the surfaces this appears on
 * failed because the API could not be reached, and a soft refresh re-runs the
 * same RSC request through the same client. A full document load also drops
 * whatever stale module state the failed render left behind.
 */
export function ReloadButton({ children }: { children?: string }) {
  const t = useTranslations('common');
  return (
    <Button
      variant="primary"
      icon={RefreshIcon}
      onClick={() => window.location.reload()}
    >
      {children ?? t('reload')}
    </Button>
  );
}
