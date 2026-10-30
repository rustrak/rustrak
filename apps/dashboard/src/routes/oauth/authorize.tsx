import type { AuthorizationRequest } from '@rustrak/client';
import { noop, useQuery } from '@tanstack/react-query';
import { createFileRoute, redirect } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { mcpQueries } from '@/features/mcp/api/queries';
import { ConsentForm } from '@/features/mcp/ui/components/consent-form';
import { returnPath } from '@/shared/api/return-path';
import { session } from '@/shared/api/session';
import { OutageScreen } from '@/shared/ui/components/outage-screen';
import { RustrakWordmark } from '@/shared/ui/components/rustrak-wordmark';
import { AuthScreenLoading } from '../-components/auth-screen-loading';

const text = (value: unknown) => (typeof value === 'string' ? value : '');
const optional = (value: unknown) =>
  typeof value === 'string' ? value : undefined;

/**
 * `/oauth/authorize`, the authorization endpoint the server advertises: where
 * an assistant such as claude.ai sends the user to approve its access to
 * `/mcp`.
 *
 * Outside `_authenticated`, so the consent page stands on its own without the
 * dashboard's header around it. It guards itself the same way: a signed-out
 * visitor leaves the address for `/` to resume, whichever way the sign-in
 * went.
 */
export const Route = createFileRoute('/oauth/authorize')({
  validateSearch: (search: Record<string, unknown>): AuthorizationRequest => ({
    response_type: text(search.response_type),
    client_id: text(search.client_id),
    redirect_uri: text(search.redirect_uri),
    code_challenge: text(search.code_challenge),
    code_challenge_method: text(search.code_challenge_method),
    state: optional(search.state),
    resource: optional(search.resource),
  }),
  beforeLoad: async ({ location }) => {
    const answer = await session.ensure();
    if (answer.state === 'anonymous') {
      // If storage is refused (private mode), signing in lands on the
      // projects list instead, and the assistant can simply start over.
      returnPath.remember(location.href);
      throw redirect({ to: '/login' });
    }
    return { answer };
  },
  loaderDeps: ({ search }) => search,
  loader: ({ deps, context: { answer, queryClient } }) => {
    if (answer.state === 'authenticated') {
      void queryClient.query(mcpQueries.authorization(deps)).catch(noop);
    }
  },
  component: AuthorizePage,
});

function AuthorizePage() {
  const t = useTranslations('oauth');
  return (
    <>
      <title>{t('meta.title')}</title>
      <Authorize />
    </>
  );
}

// `useQuery`, not a suspending read: a request the server refuses is this
// page's own answer, said in place of the consent form.
function Authorize() {
  const t = useTranslations('oauth');
  const { answer } = Route.useRouteContext();
  const request = Route.useSearch();
  const client = useQuery({
    ...mcpQueries.authorization(request),
    enabled: answer.state === 'authenticated',
  });

  if (answer.state === 'unavailable') {
    return <OutageScreen error={answer.error} />;
  }
  if (client.isLoading) return <AuthScreenLoading wide />;

  return (
    <div className="min-h-screen bg-card flex items-center justify-center p-8 lg:p-12">
      <div className="w-full max-w-[460px] space-y-10">
        <RustrakWordmark className="h-[22px] w-auto" />

        {client.data && answer.state === 'authenticated' ? (
          <ConsentForm
            request={request}
            client={client.data}
            email={answer.user.email}
          />
        ) : (
          <div className="space-y-2">
            <h1 className="text-3xl font-bold tracking-tight">
              {t('failedTitle')}
            </h1>
            <p className="text-muted-foreground">{t('failedDescription')}</p>
            {client.isError && (
              <p className="text-sm font-mono text-muted-foreground">
                {client.error.error.message}
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
