import { noop, useQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { userQueries } from '@/features/user/api/queries';
import { Link } from '@/shared/ui/components/link';
import { OutageScreen } from '@/shared/ui/components/outage-screen';
import { RustrakWordmark } from '@/shared/ui/components/rustrak-wordmark';
import { Button } from '@/shared/ui/components/shadcn/button';
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
    <div className="min-h-screen bg-card flex items-center justify-center p-8 lg:p-12">
      <div className="w-full max-w-[420px] space-y-10">
        <div className="flex items-center">
          <RustrakWordmark className="h-[22px] w-auto" />
        </div>

        {link.data ? (
          <LinkAccountForm
            email={link.data.email}
            provider={link.data.provider_name}
          />
        ) : (
          <div className="space-y-6">
            <div className="space-y-2">
              <h1 className="text-3xl font-bold tracking-tight">
                {t('link.expiredTitle')}
              </h1>
              <p className="text-muted-foreground">
                {t('link.expiredDescription')}
              </p>
            </div>
            <Button nativeButton={false} render={<Link href="/login" />}>
              {t('link.backToLogin')}
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
