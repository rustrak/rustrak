import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, Outlet } from '@tanstack/react-router';
import { projectQueries } from '@/features/project/api/queries';
import { ProjectSidebar } from '@/features/project/ui/components/project-sidebar';
import {
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from '@/shared/ui/components/shadcn/sidebar';

/**
 * Whether the sidebar was left open.
 *
 * The same `sidebar_state` cookie shadcn's `SidebarProvider` writes when the
 * reader toggles it — read here so the first paint matches what they left,
 * rather than expanding and then collapsing once the provider mounts. Under
 * Next this was `cookies()` on the server; in the browser it is the browser's
 * own copy of the same cookie, which is where the provider put it.
 */
function sidebarWasOpen(): boolean {
  return !document.cookie
    .split('; ')
    .some((entry) => entry === 'sidebar_state=false');
}

export const Route = createFileRoute('/_authenticated/projects/$id')({
  // Parsed once here, so every route below reads `id` as a number.
  params: {
    parse: ({ id }) => ({ id: Number.parseInt(id, 10) }),
    stringify: ({ id }) => ({ id: String(id) }),
  },
  loader: ({ params, context: { queryClient } }) =>
    Promise.all([
      queryClient.ensureQueryData(projectQueries.detail(params.id)),
      queryClient.ensureQueryData(projectQueries.all()),
    ]),
  component: ProjectLayout,
});

function ProjectLayout() {
  const { id: projectId } = Route.useParams();
  const { data: project } = useSuspenseQuery(projectQueries.detail(projectId));
  const { data: projectsResponse } = useSuspenseQuery(projectQueries.all());

  // The layout renders the chrome around whatever the page does with its own
  // failure, so neither fetch is fatal here. An empty switcher and a blank
  // mobile title are honest degradations; the page below this one is where the
  // same failure gets a surface with words on it.
  const projects = projectsResponse.success
    ? projectsResponse.data.items.map((p) => ({
        id: p.id,
        name: p.name,
        slug: p.slug,
        platform: p.platform,
      }))
    : [];

  return (
    <SidebarProvider
      defaultOpen={sidebarWasOpen()}
      className="min-h-[calc(100svh-4rem)]!"
    >
      <ProjectSidebar projectId={projectId} projects={projects} />
      <SidebarInset className="min-w-0 overflow-hidden">
        {/* Mobile-only bar — opens the sidebar sheet. On desktop the sidebar
            collapses via its footer button, drag-rail, or Cmd/Ctrl+B.
            top-0: it pins to the top of SidebarInset, which already sits below
            the global header (an offset like top-16 would double-shift it). */}
        <div className="sticky top-0 z-30 flex h-11 shrink-0 items-center gap-2 border-b bg-background/80 px-3 backdrop-blur-md md:hidden">
          <SidebarTrigger className="text-muted-foreground" />
          <span className="truncate text-sm font-medium text-muted-foreground">
            {project.success ? project.data.name : ''}
          </span>
        </div>
        <Outlet />
      </SidebarInset>
    </SidebarProvider>
  );
}
