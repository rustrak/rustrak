import type { Product, UserResponse } from '@rustrak/client';
import {
  AgentsIcon,
  AppearanceIcon,
  BackIcon,
  Breadcrumbs,
  CronsIcon,
  type Crumb,
  cn,
  DocsIcon,
  FacetsIcon,
  FeedbackIcon,
  type IconComponent,
  InfoIcon,
  IntegrationsIcon,
  IssuesIcon,
  LinkIcon,
  LogsIcon,
  MemberIcon,
  Menu,
  type MenuAction,
  NewIcon,
  NotificationIcon,
  OverviewIcon,
  PerformanceIcon,
  ProjectsIcon,
  ReleasesIcon,
  SettingsIcon,
  Sidebar,
  SidebarBrand,
  SidebarGroup,
  SidebarItem,
  SidebarProject,
  SidebarScope,
  SidebarSearch,
  SidebarSubmenu,
  SidebarUser,
  SignOutIcon,
  StorageIcon,
  TeamIcon,
  TokensIcon,
} from '@rustrak/ui';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useMatch } from '@tanstack/react-router';
import { PlatformIcon } from 'platformicons';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { toCommandProjects } from '@/features/project/lib/command-items';
import {
  keepsProduct,
  orderedProducts,
} from '@/features/project/model/products';
import { userMutations } from '@/features/user/api/mutations';
import { COMMAND_BAR_PROJECT_LIMIT } from '@/shared/config/commands';
import { isSettingsPath } from '@/shared/lib/settings-path';
import { CommandBar } from '@/shared/ui/components/command-bar/command-bar';
import { Link } from '@/shared/ui/components/link';
import {
  type TrailCrumb,
  usePageTrailCrumbs,
} from '@/shared/ui/hooks/use-page-trail';
import { usePathname } from '@/shared/ui/hooks/use-pathname';
import { useRouter } from '@/shared/ui/hooks/use-router';

/**
 * The frame every authenticated screen sits in: the sidebar, and the trail at
 * the top of the sheet.
 *
 * It is assembled here rather than in a feature because it spans several: the
 * project switcher is `project`, the account is `user`, and the command bar is
 * both plus the static settings routes.
 *
 * The sidebar has three modes and the URL picks between them, each one level
 * deeper than the last: the workspace (the projects), a project (its routes),
 * and settings (every settings page, the instance's and each project's).
 * Moving between them is a `SidebarScope` change, so it slides.
 */

const DOCS_URL = 'https://rustrak.dev';

/** How many projects the workspace sidebar lists under its heading. */
const SIDEBAR_PROJECTS = 5;

interface FrameProject {
  id: number;
  name: string;
  slug: string;
  platform: string | null;
  disabled_products: Product[];
  product_order: Product[];
}

/** The product a route shows, when it is one a project can turn off. */
const PRODUCT_OF: Partial<Record<NavKey, Product>> = {
  issues: 'issues',
  feedback: 'feedback',
  releases: 'releases',
  performance: 'performance',
  logs: 'logs',
  crons: 'crons',
  agents: 'agents',
};

interface SettingsEntry {
  key: string;
  icon: IconComponent;
  href: string;
  adminOnly?: boolean;
}

const ACCOUNT_SETTINGS: SettingsEntry[] = [
  { key: 'profile', icon: MemberIcon, href: '/settings/profile' },
  { key: 'preferences', icon: AppearanceIcon, href: '/settings/preferences' },
];

const WORKSPACE_SETTINGS: SettingsEntry[] = [
  { key: 'team', icon: TeamIcon, href: '/settings/team', adminOnly: true },
  {
    key: 'integrations',
    icon: IntegrationsIcon,
    href: '/settings/integrations',
  },
  { key: 'apiTokens', icon: TokensIcon, href: '/settings/tokens' },
  { key: 'mcp', icon: AgentsIcon, href: '/settings/mcp' },
  {
    key: 'storage',
    icon: StorageIcon,
    href: '/settings/storage',
    adminOnly: true,
  },
  { key: 'instance', icon: InfoIcon, href: '/settings/instance' },
];

/** A project's settings pages, appended to `/projects/$id/settings`. */
const PROJECT_SETTINGS: SettingsEntry[] = [
  { key: 'general', icon: SettingsIcon, href: '/general' },
  { key: 'data', icon: FacetsIcon, href: '/data' },
  { key: 'clientKeys', icon: TokensIcon, href: '/client-keys' },
  { key: 'members', icon: TeamIcon, href: '/members' },
  { key: 'github', icon: LinkIcon, href: '/github' },
];

type NavKey =
  | 'overview'
  | 'issues'
  | 'feedback'
  | 'releases'
  | 'performance'
  | 'logs'
  | 'crons'
  | 'alerts'
  | 'agents';

interface NavRoute {
  key: NavKey;
  icon: IconComponent;
  /** Appended to `/projects/$id`. */
  path: string;
}

const OVERVIEW: NavRoute = { key: 'overview', icon: OverviewIcon, path: '' };

const GROUPS: { label: 'groupTriage' | 'groupMonitor'; routes: NavRoute[] }[] =
  [
    {
      label: 'groupTriage',
      routes: [
        { key: 'issues', icon: IssuesIcon, path: '/issues' },
        { key: 'feedback', icon: FeedbackIcon, path: '/feedback' },
        { key: 'releases', icon: ReleasesIcon, path: '/releases' },
      ],
    },
    {
      label: 'groupMonitor',
      routes: [
        { key: 'performance', icon: PerformanceIcon, path: '/performance' },
        { key: 'logs', icon: LogsIcon, path: '/logs' },
        { key: 'crons', icon: CronsIcon, path: '/crons' },
        { key: 'alerts', icon: NotificationIcon, path: '/alerts' },
        { key: 'agents', icon: AgentsIcon, path: '/agents' },
      ],
    },
  ];

const ROUTES = [OVERVIEW, ...GROUPS.flatMap((group) => group.routes)];

/** The route the URL is in. Overview only on its exact path. */
function activeRoute(pathname: string, base: string): NavRoute | undefined {
  return ROUTES.find((route) =>
    route.path === ''
      ? pathname === base || pathname === `${base}/`
      : pathname.startsWith(`${base}${route.path}`),
  );
}

/** The project the URL is in, as the project layout's loader cached it. */
function useCurrentProject(): FrameProject | null {
  const t = useTranslations('projects');
  const match = useMatch({
    from: '/_authenticated/projects/$id',
    shouldThrow: false,
  });
  const id = match ? Number.parseInt(match.params.id, 10) : null;
  const read = useQuery({
    ...projectQueries.detail(id ?? 0),
    enabled: id !== null,
  });

  if (id === null) return null;
  if (read.data) return read.data;

  // The layout renders the chrome around whatever the page does with its own
  // failure, so an unreadable project is an honest placeholder, not an error.
  return {
    id,
    name: t('switcher.fallback'),
    slug: '',
    platform: null,
    disabled_products: [],
    product_order: [],
  };
}

/**
 * Which of the three modes the sidebar is in, and its `SidebarScope` key and
 * depth: workspace 0, a project 1, settings 2.
 */
function sidebarScope(pathname: string, project: FrameProject | null) {
  if (isSettingsPath(pathname)) {
    return { settingsMode: true, scope: 'settings', depth: 2 };
  }
  if (project) {
    return { settingsMode: false, scope: `project:${project.id}`, depth: 1 };
  }
  return { settingsMode: false, scope: 'workspace', depth: 0 };
}

/** The project's platform logo, the same one the projects list shows. */
function ProjectLogo({
  project,
  size,
}: {
  project: Pick<FrameProject, 'platform'>;
  size: 18 | 26;
}) {
  return (
    <PlatformIcon
      platform={project.platform ?? 'other'}
      size={size}
      format="lg"
      // An 18 px logo in a row built for 16 px icons: without `max-w-none`
      // the collapsed rail's 16 px content box squeezes it, and the negative
      // margin keeps its footprint at 16 so it lines up with the icons.
      className={cn('shrink-0 rounded-md', size === 18 && 'max-w-none -mx-px')}
    />
  );
}

export function AppSidebar({ user }: { user: UserResponse }) {
  const t = useTranslations('projects');
  const tUser = useTranslations('user');
  const tCommands = useTranslations('commands');
  const pathname = usePathname();
  const router = useRouter();
  const project = useCurrentProject();
  const logout = useMutation(userMutations.logout());
  const signingOut = logout.isPending;

  // One read for the switcher and the command bar; creating or deleting a
  // project invalidates it. A failed read still renders both: the static
  // commands are the bulk of the bar, and an empty switcher still leads to
  // "All projects".
  const read = useQuery(
    projectQueries.list({ per_page: COMMAND_BAR_PROJECT_LIMIT }),
  );
  const projects = read.data?.items ?? [];

  const base = project ? `/projects/${project.id}` : '';
  const { settingsMode, scope, depth } = sidebarScope(pathname, project);
  const isAdmin = user.role === 'admin';
  const switcher: MenuAction[] = [
    {
      id: 'all',
      label: t('switcher.all'),
      icon: ProjectsIcon,
      onSelect: () => router.push('/projects'),
    },
    ...projects.map((option, index) => ({
      id: String(option.id),
      label: option.name,
      leading: <ProjectLogo project={option} size={18} />,
      selected: option.id === project?.id,
      separated: index === 0,
      onSelect: () => router.push(`/projects/${option.id}`),
    })),
    {
      id: 'new',
      label: t('switcher.new'),
      icon: NewIcon,
      separated: true,
      onSelect: () => router.push('/projects/new'),
    },
  ];

  const signOut = () =>
    logout.mutate(undefined, { onSettled: () => router.push('/login') });

  return (
    <Sidebar
      header={
        <>
          {/* Inside a project the top row is its switcher; above them all
              there is nothing to switch, and the row is the mark. */}
          <SidebarBrand
            render={<Link href="/projects" />}
            switcher={
              project && !settingsMode ? (
                <Menu
                  trigger={
                    <SidebarProject
                      name={project.name}
                      detail={project.slug}
                      tile={<ProjectLogo project={project} size={18} />}
                    />
                  }
                  actions={switcher}
                  popupClassName="w-64"
                />
              ) : undefined
            }
          />
          <CommandBar
            projects={toCommandProjects(projects)}
            trigger={(open, shortcut) => (
              <SidebarSearch
                label={tCommands('triggerSearch')}
                shortcut={shortcut}
                onClick={open}
              />
            )}
          />
        </>
      }
      footer={
        <>
          {/* Inside settings the way out is the back row at the top, and the
              settings link would point at the page you are on. */}
          {settingsMode ? null : (
            <FooterLinks
              settingsHref={
                project ? `${base}/settings/general` : '/settings/profile'
              }
            />
          )}
          <SidebarUser
            name={user.email}
            detail={tUser(`roles.${user.role}`)}
            actions={[
              {
                id: 'settings',
                label: tUser('header.settings'),
                icon: SettingsIcon,
                onSelect: () => router.push('/settings/profile'),
              },
              {
                id: 'sign-out',
                label: signingOut
                  ? tUser('header.signingOut')
                  : tUser('header.signOut'),
                icon: SignOutIcon,
                tone: 'danger',
                separated: true,
                disabled: signingOut,
                onSelect: signOut,
              },
            ]}
          />
        </>
      }
    >
      {/* Workspace and project are two depths of one list: going into a
          project slides its routes in from the right, leaving it slides the
          workspace back in from the left. */}
      <SidebarScope scope={scope} depth={depth}>
        {settingsMode ? (
          <SettingsNav
            project={project}
            projects={projects}
            isAdmin={isAdmin}
          />
        ) : project ? (
          <ProjectNav project={project} />
        ) : (
          <WorkspaceNav projects={projects} />
        )}
      </SidebarScope>
    </Sidebar>
  );
}

type ProjectOption =
  | FrameProject
  | { id: number; name: string; platform: string | null };

/** Every settings page: the account, the workspace, and each project's. */
function SettingsNav({
  project,
  projects,
  isAdmin,
}: {
  project: FrameProject | null;
  projects: ProjectOption[];
  isAdmin: boolean;
}) {
  const t = useTranslations('projects');
  const tSettings = useTranslations('settings');
  const pathname = usePathname();
  const base = project ? `/projects/${project.id}` : '';
  // The list is capped; the project you are in is listed whether it made the
  // cut or not, since its settings pages have no other way in.
  const listed =
    project && !projects.some((p) => p.id === project.id)
      ? [project, ...projects]
      : projects;
  return (
    <>
      <SidebarItem
        icon={BackIcon}
        label={project ? project.name : t('switcher.all')}
        render={<Link href={project ? base : '/projects'} />}
      />
      <SidebarGroup label={tSettings('nav.groupAccount')}>
        {ACCOUNT_SETTINGS.map((entry) => (
          <SettingsLink key={entry.key} entry={entry} pathname={pathname} />
        ))}
      </SidebarGroup>
      <SidebarGroup label={tSettings('nav.groupWorkspace')}>
        {WORKSPACE_SETTINGS.filter((e) => !e.adminOnly || isAdmin).map(
          (entry) => (
            <SettingsLink key={entry.key} entry={entry} pathname={pathname} />
          ),
        )}
      </SidebarGroup>
      {listed.length > 0 ? (
        <SidebarGroup label={t('switcher.projects')}>
          {listed.map((option) => {
            const optionBase = `/projects/${option.id}/settings`;
            return (
              <SidebarSubmenu
                key={option.id}
                label={option.name}
                leading={<ProjectLogo project={option} size={18} />}
                defaultOpen={option.id === project?.id}
              >
                {PROJECT_SETTINGS.map((entry) => (
                  <SettingsLink
                    key={entry.key}
                    entry={{
                      ...entry,
                      href: `${optionBase}${entry.href}`,
                    }}
                    pathname={pathname}
                  />
                ))}
              </SidebarSubmenu>
            );
          })}
        </SidebarGroup>
      ) : null}
    </>
  );
}

/** A project's routes, in its own order, without the products it turned off. */
function ProjectNav({ project }: { project: FrameProject }) {
  const t = useTranslations('projects');
  const pathname = usePathname();
  const base = `/projects/${project.id}`;
  const active = activeRoute(pathname, base);
  const productRank = orderedProducts(project.product_order);
  // Alerts belongs to no product and is always there.
  const shown = (route: NavRoute) => {
    const product = PRODUCT_OF[route.key];
    return !product || keepsProduct(project.disabled_products, product);
  };
  const rank = (route: NavRoute) => {
    const product = PRODUCT_OF[route.key];
    return product ? productRank.indexOf(product) : productRank.length;
  };
  return (
    <>
      <SidebarItem
        icon={OVERVIEW.icon}
        label={t('nav.overview')}
        active={active === OVERVIEW}
        render={<Link href={base} />}
      />
      {GROUPS.map((group) => (
        <SidebarGroup key={group.label} label={t(`nav.${group.label}`)}>
          {group.routes
            .filter(shown)
            .sort((a, b) => rank(a) - rank(b))
            .map((route) => (
              <SidebarItem
                key={route.key}
                icon={route.icon}
                label={t(`nav.${route.key}`)}
                active={active === route}
                render={<Link href={`${base}${route.path}`} />}
              />
            ))}
        </SidebarGroup>
      ))}
    </>
  );
}

/** Above any one project: the list of them, the team, and the projects. */
function WorkspaceNav({ projects }: { projects: ProjectOption[] }) {
  const t = useTranslations('projects');
  const tSettings = useTranslations('settings');
  const pathname = usePathname();
  return (
    <>
      <SidebarItem
        icon={ProjectsIcon}
        label={t('switcher.all')}
        active={pathname === '/projects'}
        render={<Link href="/projects" />}
      />
      <SidebarItem
        icon={TeamIcon}
        label={tSettings('nav.team')}
        active={pathname.startsWith('/settings/team')}
        render={<Link href="/settings/team" />}
      />
      {projects.length > 0 ? (
        <SidebarGroup label={t('switcher.projects')}>
          {/* ponytail: the first few, until settings lets the reader
                    choose which ones are pinned here. */}
          {projects.slice(0, SIDEBAR_PROJECTS).map((option) => (
            <SidebarItem
              key={option.id}
              leading={<ProjectLogo project={option} size={18} />}
              label={option.name}
              render={<Link href={`/projects/${option.id}`} />}
            />
          ))}
        </SidebarGroup>
      ) : null}
    </>
  );
}

/** Settings and the docs, at the foot of the workspace and project modes. */
function FooterLinks({ settingsHref }: { settingsHref: string }) {
  const t = useTranslations('projects');
  return (
    <>
      <SidebarItem
        icon={SettingsIcon}
        label={t('nav.settings')}
        render={<Link href={settingsHref} />}
      />
      <SidebarItem
        icon={DocsIcon}
        label={t('nav.docs')}
        render={
          <Link href={DOCS_URL} target="_blank" rel="noopener noreferrer" />
        }
      />
    </>
  );
}

/** One settings page in the sidebar's settings mode. */
function SettingsLink({
  entry,
  pathname,
}: {
  entry: SettingsEntry;
  pathname: string;
}) {
  const tSettings = useTranslations('settings');
  return (
    <SidebarItem
      icon={entry.icon}
      label={tSettings(`nav.${entry.key}` as 'nav.account')}
      // A page with pages under it -- an integration's own -- stays marked.
      active={pathname === entry.href || pathname.startsWith(`${entry.href}/`)}
      render={<Link href={entry.href} />}
    />
  );
}

/**
 * The section's crumb, then whatever the page says it is (a release's
 * version, an endpoint's name). Below the section's own page, the section
 * crumb is the way back to it.
 */
function sectionCrumbs(
  pathname: string,
  base: string,
  route: NavRoute | undefined,
  label: string,
  trail: TrailCrumb[],
): Crumb[] {
  const section = route?.path ? `${base}${route.path}` : null;
  const below = section ? pathname.length > section.length + 1 : false;
  if (!section || !below) return [{ label }];
  return [{ label, render: <Link href={section} /> }, ...trailCrumbs(trail)];
}

/** What a page below its section said it is, after that section's crumb. */
function trailCrumbs(trail: TrailCrumb[]): Crumb[] {
  return trail.map((crumb) => ({
    label: crumb.label,
    mono: crumb.mono,
    render: crumb.href ? <Link href={crumb.href} /> : undefined,
  }));
}

/** Where you are, at the top of the sheet. */
export function AppBreadcrumbs() {
  const t = useTranslations('projects');
  const tSettings = useTranslations('settings');
  const pathname = usePathname();
  const project = useCurrentProject();
  const trail = usePageTrailCrumbs();

  let items: Crumb[];

  const base = project ? `/projects/${project.id}` : '';
  // The settings page the URL is on, for the trail's last crumb.
  const settingsPage = [
    ...ACCOUNT_SETTINGS,
    ...WORKSPACE_SETTINGS,
    ...PROJECT_SETTINGS.map((e) => ({
      ...e,
      href: `${base}/settings${e.href}`,
    })),
  ].find((e) => pathname.startsWith(e.href));
  const projectCrumb: Crumb[] = project
    ? [
        {
          label: project.name,
          icon: <ProjectLogo project={project} size={18} />,
          render: <Link href={base} />,
        },
      ]
    : [];

  // Every level of a settings trail is a way back: "Settings" to the first
  // page of its nav, the page to itself from anything opened inside it (an
  // integration), then whatever that page says it is. The last crumb is
  // never a link, so the page you are on stays plain text.
  if (settingsPage && isSettingsPath(pathname)) {
    items = [
      ...projectCrumb,
      {
        label: tSettings('nav.settingsTitle'),
        render: <Link href={`${base}/settings`} />,
      },
      {
        label: tSettings(`nav.${settingsPage.key}` as 'nav.profile'),
        render: <Link href={settingsPage.href} />,
      },
      ...trailCrumbs(trail),
    ];
  } else if (project) {
    const route = activeRoute(pathname, base);
    items = [
      ...projectCrumb,
      ...sectionCrumbs(
        pathname,
        base,
        route,
        route ? t(`nav.${route.key}`) : tSettings('nav.projectSettingsTitle'),
        trail,
      ),
    ];
  } else if (pathname.startsWith('/settings')) {
    items = [
      { label: tSettings('nav.settingsTitle') },
      ...(settingsPage
        ? [{ label: tSettings(`nav.${settingsPage.key}` as 'nav.profile') }]
        : []),
    ];
  } else if (pathname.startsWith('/projects/new')) {
    items = [
      { label: t('switcher.projects'), render: <Link href="/projects" /> },
      { label: t('switcher.new') },
    ];
  } else {
    items = [{ label: t('switcher.projects') }];
  }

  return <Breadcrumbs items={items} />;
}
