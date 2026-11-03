import { Button, Text } from '@rustrak/ui';
import { noop, useQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { userQueries } from '@/features/user/api/queries';
import { Link } from '@/shared/ui/components/link';
import { OutageScreen } from '@/shared/ui/components/outage-screen';
import { RustrakWordmark } from '@/shared/ui/components/rustrak-wordmark';
import { AuthScreenLoading } from './-components/auth-screen-loading';
import { LinkAccountForm } from './-components/link-account-form';

/**
 * Where an SSO login lands when its email already belongs to an account. The
 * server keeps the identity in the session until the account's password is
 * given, so this page only asks for that password.
 */
export const Route = createFileRoute('/link-account')({
  loader: ({ context: { queryClient } }) => {
    void queryClient.query(userQueries.ssoLink()).catch(noop);
  },
  component: LinkAccountPage,
});

function LinkAccountPage() {
  const t = useTranslations('auth');
  return (
    <>
      <title>{t('link.metaTitle')}</title>
      <LinkAccount />
    </>
  );
}

// `useQuery`, not a suspending read: `not_found` is an answer this page
// renders (the link expired), not a failure for the boundary.
function LinkAccount() {
  const t = useTranslations('auth');
  const link = useQuery(userQueries.ssoLink());

  if (link.isPending) return <AuthScreenLoading />;
  if (link.isError && link.error.error.kind !== 'not_found') {
    return <OutageScreen error={link.error.error} />;
  }

  return (
    <main className="flex min-h-screen items-center justify-center bg-canvas p-8 lg:p-12">
      <div className="flex w-full max-w-[25rem] flex-col gap-10">
        <div className="flex items-center">
          <RustrakWordmark className="h-[22px] w-auto text-fg" />
        </div>

        {link.data ? (
          <LinkAccountForm
            email={link.data.email}
            provider={link.data.provider_name}
          />
        ) : (
          <div className="flex flex-col gap-6">
            <div className="flex flex-col gap-1">
              <h1 className="text-fg text-page-title">
                {t('link.expiredTitle')}
              </h1>
              <Text render={<p />} tone="muted">
                {t('link.expiredDescription')}
              </Text>
            </div>
            <Button variant="primary" render={<Link href="/login" />}>
              {t('link.backToLogin')}
            </Button>
          </div>
        )}
      </div>
    </main>
  );
}
