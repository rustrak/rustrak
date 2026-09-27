import { RefreshCw } from 'lucide-react';
import { useTranslations } from 'use-intl';
import { Button } from '@/shared/ui/components/shadcn/button';

/**
 * A reload button for a screen rendered by a Server Component.
 *
 * `router.invalidate()` is deliberately not used: the surfaces this appears on
 * failed because the API could not be reached, and a soft refresh re-runs the
 * same RSC request through the same client. A full document load also drops
 * whatever stale module state the failed render left behind.
 */
export function ReloadButton({ children }: { children?: string }) {
  const t = useTranslations('common');
  return (
    <Button onClick={() => window.location.reload()}>
      <RefreshCw className="mr-2 size-4" />
      {children ?? t('reload')}
    </Button>
  );
}
