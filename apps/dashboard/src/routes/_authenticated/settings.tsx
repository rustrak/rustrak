import { Page } from '@rustrak/ui';
import { createFileRoute, Outlet } from '@tanstack/react-router';

export const Route = createFileRoute('/_authenticated/settings')({
  component: SettingsLayout,
});

/**
 * The settings pages' frame: the scroller. The list of
 * pages is the sidebar's, which switches to its settings mode on these
 * routes, so there is no second menu in here. Each page titles itself.
 */
function SettingsLayout() {
  return (
    <div className="flex h-full flex-col">
      <Page>
        <Outlet />
      </Page>
    </div>
  );
}
