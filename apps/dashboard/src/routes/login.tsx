import { createFileRoute } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { getSsoConfig } from '@/features/user/api/queries';
import { translator } from '@/shared/i18n/intl';
import { Link } from '@/shared/ui/components/link';
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
  loader: async () => {
    const result = await getSsoConfig();
    return result.success ? result.data : null;
  },
  head: () => {
    const t = translator('auth');
    return {
      meta: [
        { title: t('meta.title') },
        { name: 'description', content: t('meta.description') },
      ],
    };
  },
  component: LoginPage,
});

function LoginPage() {
  const t = useTranslations('auth');
  const ssoConfig = Route.useLoaderData();
  const search = Route.useSearch();
  const ssoFailed = search.error === 'sso';

  return (
    <div className="min-h-screen flex">
      {/* Left Panel - Decorative (hidden on mobile) */}
      <div className="hidden lg:flex lg:w-1/2 bg-background flex-col justify-between p-12 relative overflow-hidden">
        {/* Background gradient */}
        <div className="absolute inset-0 bg-[radial-gradient(circle_at_top_right,_hsl(var(--card)),_transparent_50%)]" />
        <div className="absolute bottom-0 left-0 right-0 h-32 bg-gradient-to-t from-background to-transparent z-10" />

        {/* Brand */}
        <Link href="/" className="relative z-20 flex items-center w-fit">
          <RustrakWordmark className="h-[22px] w-auto" />
        </Link>

        {/* Welcome message */}
        <div className="relative z-20 max-w-xl">
          <h2 className="text-6xl xl:text-7xl font-extrabold tracking-tighter leading-[1.05] mb-8">
            {t('heroTitle')}
            <span className="text-primary">.</span>
          </h2>
          <p className="text-muted-foreground text-lg font-medium leading-relaxed max-w-md">
            {t('heroDescription')}
          </p>

          {/* Stats */}
          <div className="mt-12 flex items-center gap-8">
            <div>
              <span className="text-2xl font-bold text-primary">50MB</span>
              <p className="text-sm text-muted-foreground">
                {t('statMemoryFootprint')}
              </p>
            </div>
            <div>
              <span className="text-2xl font-bold text-primary">&lt;50ms</span>
              <p className="text-sm text-muted-foreground">
                {t('statIngestionLatency')}
              </p>
            </div>
            <div>
              <span className="text-2xl font-bold text-primary">10k+</span>
              <p className="text-sm text-muted-foreground">
                {t('statEventsPerSecond')}
              </p>
            </div>
          </div>
        </div>

        {/* Footer. No version: the instance version is told to signed-in
            people only, and this page is the one surface every stranger
            reaches. The server withholds it from anonymous callers for the same
            reason, so printing it here would give away what the API will not.
            It lives on `settings/about` instead. */}
        <div className="relative z-20 flex items-end text-xs text-muted-foreground font-mono">
          <p>&copy; {new Date().getFullYear()} Rustrak</p>
        </div>
      </div>

      {/* Right Panel - Form */}
      <div className="w-full lg:w-1/2 bg-card flex items-center justify-center p-8 lg:p-12">
        <div className="w-full max-w-[420px] space-y-10">
          {/* Mobile brand (hidden on desktop) */}
          <div className="lg:hidden flex items-center mb-8">
            <RustrakWordmark className="h-[22px] w-auto" />
          </div>

          {/* Form */}
          <LoginForm ssoConfig={ssoConfig} ssoFailed={ssoFailed} />
        </div>
      </div>
    </div>
  );
}
