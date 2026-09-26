import { createFileRoute, Outlet } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { ProjectSettingsMobileNav } from './settings/-components/settings-mobile-nav';
import { ProjectSettingsNav } from './settings/-components/settings-nav';

export const Route = createFileRoute('/_authenticated/projects/$id/settings')({
  component: ProjectSettingsLayout,
});

function ProjectSettingsLayout() {
  const t = useTranslations('settings');
  const { id: projectId } = Route.useParams();

  return (
    <div className="w-full">
      {/* Mobile bar. top-11 parks it under the project layout's own mobile
          bar (h-11) instead of overlapping it. */}
      <div className="sticky top-11 z-30 flex items-center gap-3 border-b bg-background px-4 py-3 md:hidden">
        <ProjectSettingsMobileNav projectId={projectId} />
        <span className="text-sm font-bold uppercase tracking-widest text-muted-foreground">
          {t('nav.projectSettingsTitle')}
        </span>
      </div>

      <div className="flex min-h-[calc(100svh-4rem)]">
        {/* top-0, not top-16: this sits inside SidebarInset, which already
            starts below the global header. */}
        <aside className="sticky top-0 hidden h-[calc(100svh-4rem)] w-64 shrink-0 flex-col overflow-y-auto border-r border-border p-6 md:flex">
          <h2 className="mb-4 px-3 text-xs font-bold uppercase tracking-widest text-muted-foreground">
            {t('nav.projectSettingsTitle')}
          </h2>
          <ProjectSettingsNav projectId={projectId} />
        </aside>

        <div className="min-w-0 flex-1 p-4 md:p-8">
          <Outlet />
        </div>
      </div>
    </div>
  );
}
