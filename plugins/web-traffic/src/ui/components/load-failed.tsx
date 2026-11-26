import { usePluginTranslations } from '@rustrak/plugin/ui';
import { Button } from '@rustrak/ui';

/**
 * In place of a read that failed with nothing to show, so a failure never
 * reads as an empty report: says so, and offers it again.
 */
export function LoadFailed({ retry }: { retry: () => unknown }) {
  const t = usePluginTranslations();
  return (
    <div
      role="alert"
      className="flex items-center gap-3 px-5 py-6 text-meta text-fg-subtle"
    >
      <span>{t('load.failed')}</span>
      <Button size="xs" variant="ghost" onClick={() => retry()}>
        {t('load.retry')}
      </Button>
    </div>
  );
}
