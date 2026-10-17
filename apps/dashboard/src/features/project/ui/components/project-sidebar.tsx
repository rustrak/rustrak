import {
  AlertCircle,
  Bot,
  CalendarClock,
  Check,
  ChevronsLeft,
  ChevronsUpDown,
  LayoutDashboard,
  Rocket,
  ScrollText,
  Settings,
  Zap,
} from 'lucide-react';
import { PlatformIcon } from 'platformicons';
import { useTranslations } from 'use-intl';
import { cn } from '@/shared/lib/utils';
import { Link } from '@/shared/ui/components/link';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/shared/ui/components/shadcn/dropdown-menu';
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
  useSidebar,
} from '@/shared/ui/components/shadcn/sidebar';
import { TooltipProvider } from '@/shared/ui/components/shadcn/tooltip';
import { usePathname } from '@/shared/ui/hooks/use-pathname';

interface ProjectOption {
  id: number;
  name: string;
  slug: string;
  platform: string | null;
}

interface ProjectSidebarProps {
  projectId: number;
  projects: ProjectOption[];
}

function ProjectAvatar({
  platform,
  className,
  size = 32,
}: {
  platform: string | null;
  className?: string;
  size?: number;
}) {
  return (
    <PlatformIcon
      platform={platform ?? 'other'}
      size={size}
      format="lg"
      className={cn('shrink-0', className)}
    />
  );
}

/** Dokploy-style project switcher: a card that opens a project picker. */
function ProjectSwitcher({
  projectId,
  projects,
}: {
  projectId: number;
  projects: ProjectOption[];
}) {
  const t = useTranslations('projects');
  const current =
    projects.find((p) => p.id === projectId) ??
    ({
      id: projectId,
      name: t('switcher.fallback'),
      slug: '',
      platform: null,
    } as ProjectOption);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <button
            type="button"
            className="flex w-full items-center gap-2 rounded-lg border bg-sidebar-accent/40 p-2 text-left transition-colors hover:bg-sidebar-accent group-data-[collapsible=icon]:size-8 group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:border-0 group-data-[collapsible=icon]:bg-transparent group-data-[collapsible=icon]:p-0"
          />
        }
      >
        <ProjectAvatar platform={current.platform} />
        <div className="flex min-w-0 flex-1 flex-col group-data-[collapsible=icon]:hidden">
          <span className="truncate text-sm font-semibold leading-tight">
            {current.name}
          </span>
          <span className="truncate font-mono text-xs text-muted-foreground leading-tight">
            {current.slug}
          </span>
        </div>
        <ChevronsUpDown className="size-4 shrink-0 text-muted-foreground group-data-[collapsible=icon]:hidden" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-64">
        <DropdownMenuGroup>
          <DropdownMenuLabel className="text-xs text-muted-foreground">
            {t('switcher.projects')}
          </DropdownMenuLabel>
          <DropdownMenuSeparator />
          {projects.map((p) => (
            <DropdownMenuItem
              key={p.id}
              render={<Link href={`/projects/${p.id}`} />}
              className="gap-2"
            >
              <ProjectAvatar platform={p.platform} size={24} />
              <div className="flex min-w-0 flex-1 flex-col">
                <span className="truncate text-sm">{p.name}</span>
                <span className="truncate font-mono text-xs text-muted-foreground">
                  {p.slug}
                </span>
              </div>
              {p.id === projectId && (
                <Check className="size-4 shrink-0 text-primary" />
              )}
            </DropdownMenuItem>
          ))}
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function CollapseButton() {
  const { toggleSidebar } = useSidebar();
  const t = useTranslations('projects');
  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <SidebarMenuButton
          tooltip={t('collapse.expand')}
          onClick={toggleSidebar}
          isActive={false}
          className="h-9 justify-center text-muted-foreground hover:bg-sidebar-accent hover:text-foreground"
        >
          <ChevronsLeft className="transition-transform group-data-[collapsible=icon]:rotate-180" />
          <span className="group-data-[collapsible=icon]:hidden">
            {t('collapse.collapse')}
          </span>
        </SidebarMenuButton>
      </SidebarMenuItem>
    </SidebarMenu>
  );
}

export function ProjectSidebar({ projectId, projects }: ProjectSidebarProps) {
  const t = useTranslations('projects');
  const pathname = usePathname();

  const navItems = [
    {
      href: `/projects/${projectId}`,
      label: t('nav.overview'),
      icon: LayoutDashboard,
      exact: true,
    },
    {
      href: `/projects/${projectId}/issues`,
      label: t('nav.issues'),
      icon: AlertCircle,
    },
    {
      href: `/projects/${projectId}/releases`,
      label: t('nav.releases'),
      icon: Rocket,
    },
    {
      href: `/projects/${projectId}/performance`,
      label: t('nav.performance'),
      icon: Zap,
    },
    {
      href: `/projects/${projectId}/agents`,
      label: t('nav.agents'),
      icon: Bot,
    },
    {
      href: `/projects/${projectId}/logs`,
      label: t('nav.logs'),
      icon: ScrollText,
    },
    {
      href: `/projects/${projectId}/crons`,
      label: t('nav.crons'),
      icon: CalendarClock,
    },
    {
      href: `/projects/${projectId}/settings`,
      label: t('nav.settings'),
      icon: Settings,
    },
  ];

  return (
    <TooltipProvider delay={0}>
      <Sidebar collapsible="icon" className="top-16! h-[calc(100svh-4rem)]!">
        <SidebarHeader className="p-2">
          <ProjectSwitcher projectId={projectId} projects={projects} />
        </SidebarHeader>

        <SidebarContent className="py-1">
          <SidebarGroup>
            <SidebarGroupLabel>{t('navigation')}</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu className="gap-1.5">
                {navItems.map((item) => {
                  const Icon = item.icon;
                  const isActive = item.exact
                    ? pathname === item.href
                    : pathname.startsWith(item.href);

                  return (
                    <SidebarMenuItem key={item.href}>
                      <SidebarMenuButton
                        // Drive active styling ourselves for the brand-green fill.
                        isActive={false}
                        tooltip={item.label}
                        render={<Link href={item.href} />}
                        className={cn(
                          'h-10 gap-3 rounded-lg text-[0.925rem] font-medium group-data-[collapsible=icon]:justify-center',
                          isActive
                            ? 'bg-primary! font-semibold text-primary-foreground! shadow-sm hover:bg-primary! hover:text-primary-foreground!'
                            : 'text-muted-foreground hover:bg-sidebar-accent hover:text-foreground',
                        )}
                      >
                        <Icon />
                        <span className="group-data-[collapsible=icon]:hidden">
                          {item.label}
                        </span>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  );
                })}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        </SidebarContent>

        <SidebarFooter>
          <CollapseButton />
        </SidebarFooter>
        <SidebarRail />
      </Sidebar>
    </TooltipProvider>
  );
}
