import {
  Button,
  OverviewIcon,
  RefreshIcon,
  Text,
  WarningIcon,
} from '@rustrak/ui';
import { useQueryErrorResetBoundary } from '@tanstack/react-query';
import { useEffect } from 'react';
import { useTranslations } from 'use-intl';
import { ApiError } from '@/shared/api/query';
import { Link } from '@/shared/ui/components/link';
import { LoadFailure } from '@/shared/ui/components/load-failure';

/**
 * The error boundary of every route: a failed read (an {@link ApiError}) or
 * a render that threw.
 *
 * No brand panel here, unlike the router's default: this boundary renders
 * *below* the header, which already carries the Rustrak mark and the
 * navigation, so a second brand panel would compete with it and strand the
 * user in a screen that looks like a logout.
 */
export function RouteError({
  error,
  reset,
}: {
  error: unknown;
  reset: () => void;
}) {
  const t = useTranslations('errors');
  const queries = useQueryErrorResetBoundary();

  // A query that failed under this boundary is retried by the next render
  // rather than rethrown from the cache.
  useEffect(() => {
    queries.reset();
  }, [queries]);

  useEffect(() => {
    if (!(error instanceof ApiError))
      console.error('Application error:', error);
  }, [error]);

  // A read the server failed: the same surface a failed panel shows, which
  // also sends an expired session to login and a missing record to the 404.
  if (error instanceof ApiError) {
    return <LoadFailure error={error.error} title={t('main.headline')} />;
  }

  return (
    <div className="flex-1 flex items-center justify-center px-6 py-20">
      <div className="max-w-md w-full text-center space-y-6">
        <div className="flex justify-center">
          <div className="flex size-12 items-center justify-center rounded-full bg-danger-surface">
            <WarningIcon className="size-5 text-danger-fg" aria-hidden="true" />
          </div>
        </div>

        <div className="space-y-2">
          <h1 className="text-fg text-title">{t('main.headline')}</h1>
          <Text tone="subtle" render={<p />}>
            {t('main.description')}
          </Text>
        </div>

        <div className="flex justify-center gap-4">
          <Button onClick={reset} variant="primary" icon={RefreshIcon}>
            {t('tryAgain')}
          </Button>
          <Button
            variant="secondary"
            icon={OverviewIcon}
            render={<Link href="/projects" />}
          >
            {t('goToProjects')}
          </Button>
        </div>
      </div>
    </div>
  );
}
