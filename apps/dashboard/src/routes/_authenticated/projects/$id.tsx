import { createFileRoute, Outlet } from '@tanstack/react-router';
import { useTranslations } from 'use-intl';
import { getProject, getProjects } from '@/features/project/api/queries';
import { ProjectSidebar } from '@/features/project/ui/components/project-sidebar';
import { createClient } from '@/shared/api/rustrak';
import { searchString } from '@/shared/lib/search-params';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/shared/ui/components/shadcn/select';
import {
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from '@/shared/ui/components/shadcn/sidebar';
import { useRouter } from '@/shared/ui/hooks/use-router';

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
  validateSearch: (search: Record<string, unknown>) => ({
    environment: searchString(search.environment),
  }),
  loader: async ({ params }) => {
    const projectId = Number.parseInt(params.id, 10);
    const client = await createClient();
    const [project, projects, environments] = await Promise.all([
      getProject(projectId),
      getProjects({ per_page: 100 }),
      client.projects.environments(projectId),
    ]);
    return { project, projects, environments };
  },
  component: ProjectLayout,
});

function ProjectLayout() {
  const { id } = Route.useParams();
  const {
    project,
    projects: projectsResponse,
    environments,
  } = Route.useLoaderData();
  const { environment } = Route.useSearch();
  const router = useRouter();
  const t = useTranslations('agents.filters');
  const projectId = Number.parseInt(id, 10);

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
      className="h-[calc(100svh-4rem)]! min-h-0!"
    >
      <ProjectSidebar projectId={projectId} projects={projects} />
      <SidebarInset className="min-h-0 min-w-0 overflow-hidden">
        <div className="flex shrink-0 items-center gap-2 border-b px-4 py-2 md:px-8">
          <label
            htmlFor="project-environment"
            className="text-sm text-muted-foreground"
          >
            {t('environment')}
          </label>
          <Select
            value={environment ?? ''}
            onValueChange={(value) => {
              const url = new URL(window.location.href);
              if (value) url.searchParams.set('environment', String(value));
              else url.searchParams.set('environment', '');
              url.searchParams.delete('page');
              router.push(`${url.pathname}${url.search}${url.hash}`);
            }}
          >
            <SelectTrigger
              id="project-environment"
              size="sm"
              className="min-w-44 max-w-56"
            >
              <SelectValue>
                {(value) => (value ? String(value) : t('allEnvironments'))}
              </SelectValue>
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="">{t('allEnvironments')}</SelectItem>
              {Array.from(
                new Set([
                  ...(environments.success ? environments.data : []),
                  ...(environment ? [environment] : []),
                ]),
              ).map((name) => (
                <SelectItem key={name} value={name}>
                  {name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
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
        <div className="min-h-0 flex-1 overflow-y-auto">
          <Outlet />
        </div>
      </SidebarInset>
    </SidebarProvider>
  );
}
