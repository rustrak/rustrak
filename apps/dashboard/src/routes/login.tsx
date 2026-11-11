import { Skeleton, Text, Wordmark } from '@rustrak/ui';
import { noop, useQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { userQueries } from '@/features/user/api/queries';
import { Link } from '@/shared/ui/components/link';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { LoginForm } from './-components/login-form';
import { LoginOrbit } from './-components/login-orbit';

/**
 * `/login`, not `/auth/login`: `/auth` is the
 * API's namespace — `/auth/me`, `/auth/login` as a POST — and now that the
 * page and the API answer on the same origin, a page at `/auth/login` would be
 * a GET the API scope claims and has no handler for. `routes::dashboard` keeps
 * the five API prefixes for the server precisely so that a mistyped endpoint
 * stays a JSON answer rather than becoming HTML, and carving a hole in that
 * list for one page would give up the property for every other path under it.
 */
export const Route = createFileRoute('/login')({
  validateSearch: (search: Record<string, unknown>): { error?: string } => ({
    error: typeof search.error === 'string' ? search.error : undefined,
  }),
  loader: ({ context: { queryClient } }) => {
    void queryClient.query(userQueries.ssoConfig()).catch(noop);
  },
  component: LoginPage,
});

function LoginPage() {
  const t = useTranslations('auth');
  // `useQuery`, not a suspending read: a failed lookup leaves the password
  // form, and the server decides whether it is allowed. The form waits for
  // the answer, though, so it does not reshape under the reader.
  const sso = useQuery(userQueries.ssoConfig());
  const search = Route.useSearch();
  const ssoFailed = search.error === 'sso';

  return (
    <main className="flex min-h-screen bg-canvas">
      <title>{t('meta.title')}</title>

      {/* The sheet the dashboard's pages sit on, here holding only the brand,
          so signing in already happens inside the product's frame. */}
      <aside className="relative my-frame ms-frame hidden flex-[1.2] overflow-hidden rounded-3xl border border-border-subtle bg-panel lg:block">
        <LoginOrbit />
        {/* What the copy needs behind it, then the edges falling away to the
            frame's own tone, over everything but the copy. */}
        <div
          aria-hidden="true"
          className="pointer-events-none absolute inset-x-0 bottom-0 h-56 bg-linear-to-t from-panel from-40% to-transparent"
        />
        <div
          aria-hidden="true"
          className="pointer-events-none absolute inset-0 backdrop-blur-[6px] mask-[radial-gradient(ellipse_at_50%_44%,transparent_50%,#000_95%)]"
        />
        <div
          aria-hidden="true"
          className="login-vignette pointer-events-none absolute inset-0 rounded-3xl"
        />
        <div className="absolute inset-x-0 bottom-0 flex flex-col gap-2 p-10 motion-safe:animate-login-rise motion-safe:[animation-delay:300ms]">
          <p className="text-fg text-page-title">{t('panel.title')}</p>
          <Text render={<p />} tone="muted">
            {t('panel.subtitle')}
          </Text>
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col gap-10 p-8 lg:p-10">
        {/* On a wide screen the mark is at the centre of the panel. */}
        <Link
          href="/"
          className="flex w-fit motion-safe:animate-login-rise lg:hidden"
        >
          <Wordmark className="h-[22px] w-auto text-fg" />
        </Link>

        <div className="login-stagger mx-auto my-auto w-full max-w-[22rem]">
          {sso.isPending ? (
            <LoginFormLoading />
          ) : (
            <LoginForm ssoConfig={sso.data ?? null} ssoFailed={ssoFailed} />
          )}
        </div>

        <nav className="mx-auto flex items-center gap-3 motion-safe:animate-login-rise motion-safe:[animation-delay:650ms] text-meta text-fg-subtle">
          <a href="https://rustrak.dev" className="hover:text-fg">
            {t('footer.docs')}
          </a>
          <span aria-hidden="true">·</span>
          <a
            href="https://github.com/rustrak/rustrak"
            className="hover:text-fg"
          >
            GitHub
          </a>
        </nav>
      </div>
    </main>
  );
}

/** The form's heading as itself, its fields and button while SSO is asked. */
function LoginFormLoading() {
  const t = useTranslations('auth');
  return (
    <PageLoading>
      <div className="flex flex-col gap-6">
        <div className="flex flex-col gap-1">
          <h1 className="text-fg text-page-title">{t('form.title')}</h1>
          <Text render={<p />} tone="muted">
            {t('form.subtitle')}
          </Text>
        </div>
        <div className="flex flex-col gap-4">
          <Skeleton className="h-14 w-full" />
          <Skeleton className="h-14 w-full" />
          <Skeleton className="mt-2 h-10 w-full" />
        </div>
      </div>
    </PageLoading>
  );
}
