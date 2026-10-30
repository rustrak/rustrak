import { Skeleton } from '@rustrak/ui';
import { noop, useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { githubQueries } from '@/features/github/api/queries';
import { GitHubIntegration } from '@/features/github/ui/components/github-integration';
import { GitHubLoginCard } from '@/features/github/ui/components/github-login-card';
import { GitHubMark } from '@/shared/ui/components/github-mark';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { QueryState } from '@/shared/ui/components/query-state';
import { SettingsListSkeleton } from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';
import { usePageTrail } from '@/shared/ui/hooks/use-page-trail';

export const Route = createFileRoute(
  '/_authenticated/settings/integrations/github',
)({
  // The connection is the page; the reader's own login is a card that fills
  // in on its own.
  loader: ({ context: { queryClient } }) => {
    void queryClient.query(githubQueries.myLogin()).catch(noop);
    void queryClient.query(githubQueries.status()).catch(noop);
  },
  pendingComponent: GitHubIntegrationLoading,
  component: GitHubIntegrationPage,
});

/** Everything about GitHub, including which GitHub account is the reader's. */
function GitHubIntegrationPage() {
  const t = useTranslations('settings');
  const { data: status } = useSuspenseQuery(githubQueries.status());
  const login = useQuery(githubQueries.myLogin());
  usePageTrail([{ label: 'GitHub' }]);
  const loginCard = (
    <QueryState
      query={login}
      placeholder={null}
      title={t('integrations.githubLoadFailed')}
    >
      {(data) => <GitHubLoginCard login={data.login} />}
    </QueryState>
  );

  return (
    <>
      <GitHubHeader />
      {status === null ? (
        // The app and its installations are an admin's; which GitHub account
        // is theirs is every member's to set.
        loginCard
      ) : (
        <GitHubIntegration status={status} login={loginCard} />
      )}
    </>
  );
}

function GitHubIntegrationLoading() {
  return (
    <PageLoading>
      <GitHubHeader />
      <Skeleton className="mb-3 h-5 w-40" />
      <div className="overflow-hidden rounded-2xl border border-border-subtle bg-surface">
        <SettingsListSkeleton />
      </div>
    </PageLoading>
  );
}

/** The page's title, the same whether the connection has landed or not. */
function GitHubHeader() {
  const tGitHub = useTranslations('github');
  return (
    <>
      <title>GitHub | Rustrak</title>
      <SettingsHeader
        title={
          <span className="flex min-w-0 items-center gap-3">
            <span className="flex size-9 shrink-0 items-center justify-center rounded-xl border border-border-subtle text-fg">
              <GitHubMark className="size-5" />
            </span>
            GitHub
          </span>
        }
        description={tGitHub('connection.description')}
      />
    </>
  );
}
