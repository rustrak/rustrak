import { Button, RefreshIcon } from '@rustrak/ui';
import { useTranslations } from 'use-intl';

/**
 * A full document reload, for a screen shown because the API could not be
 * reached: it also drops whatever stale module state the failed load left.
 */
export function ReloadButton() {
  const t = useTranslations('common');
  return (
    <Button
      variant="primary"
      icon={RefreshIcon}
      onClick={() => window.location.reload()}
    >
      {t('reload')}
    </Button>
  );
}
