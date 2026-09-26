import { useTranslations } from 'use-intl';
import { ErrorScreen } from '@/shared/ui/components/error-screen';
import { OutageScreen } from '@/shared/ui/components/outage-screen';
import { ReloadButton } from '@/shared/ui/components/reload-button';

/** A route chunk that is gone: the bundle this tab loaded predates a deploy. */
function isStaleChunk(error: unknown): boolean {
  return (
    error instanceof Error &&
    /dynamically imported module|module script failed/i.test(error.message)
  );
}

/**
 * What the router renders for an error nothing closer caught.
 *
 * A missing chunk means the tab outlived a deploy, and a reload is the whole
 * fix. Anything else is a fault in the dashboard itself, and saying "the
 * server did not answer" for it would send the reader to check a server that
 * is fine.
 */
export function AppErrorScreen({ error }: { error: unknown }) {
  const t = useTranslations('errors.app');

  if (isStaleChunk(error)) {
    return (
      <OutageScreen
        error={{
          kind: 'network',
          reason: 'unreachable',
          message: (error as Error).message,
        }}
      />
    );
  }

  return (
    <ErrorScreen
      headline={t('headline')}
      description={t('description')}
      actions={<ReloadButton />}
    />
  );
}
