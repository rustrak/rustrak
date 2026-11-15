import { AppShell, Topbar } from '@rustrak/ui';
import { noop } from '@tanstack/react-query';
import { createFileRoute, Outlet, redirect } from '@tanstack/react-router';
import { projectQueries } from '@/features/project/api/queries';
import { TimeZoneSync } from '@/features/user/ui/components/time-zone-sync';
import { accessQueries } from '@/shared/api/permissions';
import { returnPath } from '@/shared/api/return-path';
import { session } from '@/shared/api/session';
import { OutageScreen } from '@/shared/ui/components/outage-screen';
import { PullToRefresh } from '@/shared/ui/components/pull-to-refresh';
import { TableLabels } from '@/shared/ui/components/table-labels';
import { UpdateBannerSlot } from '@/shared/ui/components/update-banner-slot';
import {
  AppBreadcrumbs,
  AppSidebar,
} from './_authenticated/-components/app-frame';
import { AppTabBar, MobileNav } from './_authenticated/-components/mobile-nav';

/**
 * The authenticated dashboard: one gate, and the chrome every screen under it
 * shares.
 *
 * **The gate is here and nowhere else.** A check inside a component has already
 * painted the thing it was protecting, so it runs in `beforeLoad`, before the
 * route's own loader and before anything renders. Adding a page means putting
 * the file under `_authenticated/`; it inherits this and needs no check.
 *
 * The route is pathless (`_authenticated`, not `authenticated`), so
 * `/projects` is still `/projects`.
 *
 * `anonymous` is the only state that redirects, and it is the branch a careless
 * port turns into a login loop. `unavailable` renders instead, and deliberately
 * does not render `<Outlet />`, because every screen below assumes it has a
 * user.
 */
export const Route = createFileRoute('/_authenticated')({
  beforeLoad: async ({ location, preload }) => {
    const answer = await session.ensure();

    if (answer.state === 'anonymous') {
      // Signing in lands on `/`, which comes back here rather than to
      // `/projects`: a link to an issue, from Slack or an alert email, should
      // open the issue. Not on a preload, which is a hovered link rather than
      // somewhere the reader went.
      if (!preload) returnPath.remember(location.href);
      throw redirect({ to: '/login' });
    }

    return answer;
  },
  // The sidebar's switcher and the command bar, started here so they are on
  // their way before the frame renders; neither waits for it.
  loader: ({ context: { queryClient } }) => {
    void queryClient.query(accessQueries.me()).catch(noop);
    void queryClient.query(projectQueries.picker()).catch(noop);
  },
  component: AuthenticatedLayout,
});

function AuthenticatedLayout() {
  const answer = Route.useRouteContext();

  // Returned bare, with no wrapper: this is the one branch that renders no
  // frame, so the screen owns the whole viewport and brings its own brand.
  if (answer.state === 'unavailable') {
    return <OutageScreen error={answer.error} />;
  }

  // On a phone the tab bar replaces the sidebar, and the "More" sheet behind
  // it holds everything the tabs do not; `MobileNav` owns that sheet.
  return (
    <MobileNav user={answer.user}>
      <AppShell
        sidebar={<AppSidebar user={answer.user} />}
        tabBar={<AppTabBar />}
        defaultCollapsed={sidebarWasCollapsed()}
        onCollapsedChange={rememberSidebar}
      >
        {/* Renders nothing. Adopts the browser's timezone onto the account the
          first time a reader arrives without one; see the component. It sits
          here rather than above the router because it needs a session, and
          this is the first place there is guaranteed to be one. */}
        <TimeZoneSync hasTimeZone={Boolean(answer.user.timezone)} />
        <Topbar>
          <AppBreadcrumbs />
        </Topbar>
        {/* The sheet's only scroller. Screens that own their scrolling fill it
          with `h-full` and scroll inside -- except on a phone, where the
          screen grows to its content and this scrolls it whole (`Page`). */}
        <PullToRefresh
          data-page-transition=""
          className="min-h-0 flex-1 overflow-y-auto bg-panel max-md:[&>:not([aria-hidden])]:h-auto max-md:[&>:not([aria-hidden])]:min-h-full"
        >
          <TableLabels>
            <Outlet />
          </TableLabels>
        </PullToRefresh>
        {/* Fixed-positioned, so arriving late shifts nothing. */}
        <UpdateBannerSlot />
      </AppShell>
    </MobileNav>
  );
}

/**
 * Whether the sidebar was left collapsed, read synchronously so the first
 * paint matches what the reader left rather than expanding and then snapping
 * shut. The cookie is the one shadcn's sidebar used to write, so the choice
 * survives the redesign.
 */
function sidebarWasCollapsed(): boolean {
  return document.cookie
    .split('; ')
    .some((entry) => entry === 'sidebar_state=false');
}

function rememberSidebar(collapsed: boolean) {
  // biome-ignore lint/suspicious/noDocumentCookie: one boolean, read before the first paint; the Cookie Store API is async.
  document.cookie = `sidebar_state=${!collapsed}; path=/; max-age=31536000; SameSite=Lax`;
}
