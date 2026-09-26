import type { ErrorComponentProps } from '@tanstack/react-router';
import {
  createFileRoute,
  Link,
  Outlet,
  redirect,
} from '@tanstack/react-router';
import { AlertTriangle, Home, RefreshCw } from 'lucide-react';
import { useEffect } from 'react';
import { useTranslations } from 'use-intl';
import { Header } from '@/features/user/ui/components/header';
import { TimeZoneSync } from '@/features/user/ui/components/time-zone-sync';
import { session } from '@/shared/api/session';
import { OutageScreen } from '@/shared/ui/components/outage-screen';
import { Button } from '@/shared/ui/components/shadcn/button';
import { UpdateBannerSlot } from '@/shared/ui/components/update-banner-slot';
import { CommandBarSlot } from './_authenticated/-components/command-bar-slot';

/**
 * The authenticated dashboard: one gate, and the chrome every screen under it
 * shares.
 *
 * **The gate is here and nowhere else.** A check inside a component has already
 * painted the thing it was protecting, so it runs in `beforeLoad`, before the
 * route's own loader and before anything renders. Adding a page means putting
 * the file under `_authenticated/`; it inherits this and needs no check.
 *
 * The path is pathless (`_authenticated`, not `authenticated`), so `/projects`
 * is still `/projects`. It is the same trick Next's `(main)` route group was
 * doing.
 *
 * `anonymous` is the only state that redirects, and it is the branch a careless
 * port turns into a login loop. `unavailable` renders instead, and deliberately
 * does not render `<Outlet />`, because every screen below assumes it has a
 * user.
 */
export const Route = createFileRoute('/_authenticated')({
  beforeLoad: async ({ location }) => {
    const answer = await session.ensure();

    if (answer.state === 'anonymous') {
      // Signing in comes back here, not to the project list.
      throw redirect({ to: '/login', search: { redirect: location.href } });
    }

    return answer;
  },
  component: AuthenticatedLayout,
  errorComponent: MainError,
});

function AuthenticatedLayout() {
  const answer = Route.useRouteContext();

  // Returned bare, with no wrapper: this is the one branch that renders no
  // `Header`, so the screen owns the whole viewport and brings its own brand.
  if (answer.state === 'unavailable') {
    return <OutageScreen error={answer.error} />;
  }

  return (
    <div className="min-h-screen flex flex-col">
      {/* Renders nothing. Adopts the browser's timezone onto the account the
          first time a reader arrives without one; see the component. It sits
          here rather than above the router because it needs a session, and
          this is the first place there is guaranteed to be one. */}
      <TimeZoneSync hasTimeZone={Boolean(answer.user.timezone)} />
      <Header user={answer.user} commandBar={<CommandBarSlot />} />
      <main className="flex-1">
        <Outlet />
      </main>
      {/* Fixed-positioned, so arriving late shifts nothing. */}
      <UpdateBannerSlot />
    </div>
  );
}

/**
 * The error boundary for everything under the header.
 *
 * No brand panel here, unlike the router's default: this boundary renders
 * *below* the header, which already carries the Rustrak mark and the
 * navigation, so a second brand panel would compete with it and strand the
 * user in a screen that looks like a logout.
 */
function MainError({ error, reset }: ErrorComponentProps) {
  const t = useTranslations('errors');

  useEffect(() => {
    console.error('Application error:', error);
  }, [error]);

  return (
    <div className="flex-1 flex items-center justify-center px-6 py-20">
      <div className="max-w-md w-full text-center space-y-6">
        <div className="flex justify-center">
          <div className="size-12 rounded-full bg-destructive/10 flex items-center justify-center">
            <AlertTriangle
              className="size-5 text-destructive"
              aria-hidden="true"
            />
          </div>
        </div>

        <div className="space-y-2">
          <h1 className="text-xl font-bold tracking-tight">
            {t('main.headline')}
          </h1>
          <p className="text-muted-foreground text-sm leading-relaxed">
            {t('main.description')}
          </p>
        </div>

        <div className="flex justify-center gap-4">
          <Button onClick={reset} variant="default">
            <RefreshCw className="mr-2 size-4" />
            {t('tryAgain')}
          </Button>
          <Button
            variant="outline"
            nativeButton={false}
            render={<Link to="/projects" />}
          >
            <Home className="mr-2 size-4" />
            {t('goToProjects')}
          </Button>
        </div>
      </div>
    </div>
  );
}
