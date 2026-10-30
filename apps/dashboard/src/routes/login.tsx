import { Skeleton, Text } from '@rustrak/ui';
import { noop, useQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { userQueries } from '@/features/user/api/queries';
import { Link } from '@/shared/ui/components/link';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { RustrakWordmark } from '@/shared/ui/components/rustrak-wordmark';
import { LoginForm } from './-components/login-form';

/**
 * `/login`, where the Next app had `/auth/login`.
 *
 * **The one address this migration changes, and it had to.** `/auth` is the
 * API's namespace — `/auth/me`, `/auth/login` as a POST — and now that the
 * page and the API answer on the same origin, a page at `/auth/login` would be
 * a GET the API scope claims and has no handler for. `routes::dashboard` keeps
 * the five API prefixes for the server precisely so that a mistyped endpoint
 * stays a JSON answer rather than becoming HTML, and carving a hole in that
 * list for one page would give up the property for every other path under it.
 *
 * Nothing else moved: `/projects`, `/settings/...`, `/invite/<token>` and every
 * project route are the addresses they were.
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
    <main className="relative flex min-h-screen flex-col items-center justify-center overflow-hidden bg-canvas px-4 py-12">
      <title>{t('meta.title')}</title>
      {/* A directional light: a beam from just above the top-right, angled
          down towards the card. A wide soft cone and a narrow brighter core,
          both hung from their top edge so rotating them swings the beam
          around its source. Each is placed by its centre from the right edge, so
          on a phone the source stays in the corner, and sized in `lvh`,
          which does not change as a phone's URL bar folds away. */}
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-0 overflow-hidden"
      >
        <div className="absolute -top-[15lvh] left-[calc(100%-2.5rem)] sm:left-[calc(100%-23vw)] h-[180lvh] w-[max(36vw,260px)] -translate-x-1/2 origin-top rotate-[28deg] motion-safe:animate-login-beam bg-[radial-gradient(ellipse_50%_85%_at_50%_0%,rgb(255_255_255/0.08),transparent_90%)] blur-2xl" />
        <div className="absolute -top-[15lvh] left-[calc(100%-4rem)] sm:left-[calc(100%-22vw)] h-[160lvh] w-[max(14vw,110px)] -translate-x-1/2 origin-top rotate-[28deg] motion-safe:animate-login-beam motion-safe:[animation-delay:150ms] bg-[radial-gradient(ellipse_50%_85%_at_50%_0%,rgb(255_255_255/0.07),transparent_90%)] blur-xl" />
      </div>

      {/* Shadow: everything the beam does not reach falls darker, deepest
          in the bottom-left corner opposite the source. */}
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-0 motion-safe:animate-login-fade bg-[radial-gradient(ellipse_75%_70%_at_70%_15%,transparent_30%,rgb(0_0_0/0.55)_100%)]"
      />

      <div className="relative flex w-full max-w-[25rem] flex-col items-center gap-8">
        <Link href="/" className="flex w-fit motion-safe:animate-login-rise">
          <RustrakWordmark className="h-[22px] w-auto text-fg" />
        </Link>

        {/* The card where the beam lands. The light comes from the top-right,
            so everything below is lit from that corner: a border that
            brightens towards it, a highlight along the top edge, a sheen
            inside, and a faint bounce off the far corner. */}
        <div className="relative isolate w-full motion-safe:animate-login-rise motion-safe:[animation-delay:100ms]">
          {/* Spill: light that misses the card and pools around it. */}
          <div
            aria-hidden="true"
            className="pointer-events-none absolute -inset-16 -z-10 motion-safe:animate-login-fade motion-safe:[animation-delay:500ms] bg-[radial-gradient(ellipse_60%_50%_at_80%_10%,rgb(255_255_255/0.03),transparent_70%)] blur-2xl"
          />

          {/* The border is a 1px gradient frame: bright at the lit corner,
              falling back to the ordinary border opposite it. */}
          <div className="relative rounded-xl bg-border-raised p-px shadow-raised">
            {/* The light on the card, revealed from the corner it comes from.
                Deliberately not masked to the ring: the card is translucent,
                and this layer showing through it is what gives the face its
                lit tone. */}
            <div
              aria-hidden="true"
              className="absolute inset-0 rounded-xl [mask-image:radial-gradient(circle_at_100%_0%,#000_45%,transparent)] [mask-position:100%_0%] [mask-repeat:no-repeat] [mask-size:400%_400%] bg-[linear-gradient(215deg,rgb(255_255_255/0.22),transparent_35%,transparent_75%,rgb(255_255_255/0.04))] motion-safe:animate-login-sweep motion-safe:[animation-delay:500ms]"
            />
            <div className="relative overflow-hidden rounded-[calc(var(--radius-xl)-1px)] bg-canvas/70 p-8">
              <div
                aria-hidden="true"
                className="pointer-events-none absolute inset-0 motion-safe:animate-login-fade motion-safe:[animation-delay:700ms] bg-[radial-gradient(ellipse_70%_55%_at_100%_0%,rgb(255_255_255/0.09),transparent_70%),radial-gradient(ellipse_50%_40%_at_0%_100%,rgb(255_255_255/0.015),transparent_70%),linear-gradient(215deg,transparent_35%,rgb(0_0_0/0.18))]"
              />
              <div
                aria-hidden="true"
                className="pointer-events-none absolute inset-x-0 top-0 h-px [mask-image:linear-gradient(to_left,#000_33%,transparent_66%)] [mask-position:100%_0%] [mask-repeat:no-repeat] [mask-size:300%_100%] motion-safe:animate-login-wipe motion-safe:[animation-delay:800ms] bg-[linear-gradient(90deg,transparent_20%,rgb(255_255_255/0.35)_85%,transparent)]"
              />
              <div className="login-stagger relative">
                {sso.isPending ? (
                  <LoginFormLoading />
                ) : (
                  <LoginForm
                    ssoConfig={sso.data ?? null}
                    ssoFailed={ssoFailed}
                  />
                )}
              </div>
            </div>
          </div>
        </div>

        <nav className="flex items-center gap-3 motion-safe:animate-login-rise motion-safe:[animation-delay:650ms] text-meta text-fg-subtle">
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
