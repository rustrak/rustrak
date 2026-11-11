import type { ProjectResponse, UserResponse } from '@rustrak/client';
import {
  BackIcon,
  Breadcrumbs,
  type Crumb,
  cn,
  DocsIcon,
  Menu,
  type MenuAction,
  NewIcon,
  ProjectsIcon,
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
  TeamIcon,
} from '@rustrak/ui';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useLocation, useMatch } from '@tanstack/react-router';
import { PlatformIcon } from 'platformicons';
import { useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { keepsProduct } from '@/features/project/model/products';
import { userMutations } from '@/features/user/api/mutations';
import type { Permissions } from '@/shared/lib/permissions';
import { Link } from '@/shared/ui/components/link';
import {
  type TrailCrumb,
  usePageTrailCrumbs,
} from '@/shared/ui/hooks/use-page-trail';
import { usePermissionsLater } from '@/shared/ui/hooks/use-permissions';
import { useRouter } from '@/shared/ui/hooks/use-router';
import { CommandBar } from './command-bar/command-bar';
import {
  ACCOUNT_SETTINGS,
  allowed,
  canSeeTeam,
  inProductOrder,
  isSettingsPath,
  isUnder,
  type NavPage,
  OVERVIEW,
  PROJECT_GROUPS,
  PROJECT_PAGES,
  PROJECT_SETTINGS,
  WORKSPACE_SETTINGS,
} from './nav';

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

type FrameProject = Pick<
  ProjectResponse,
  'id' | 'name' | 'slug' | 'platform' | 'disabled_products' | 'product_order'
>;

type ProjectOption = Pick<ProjectResponse, 'id' | 'name' | 'platform'>;

/** The path without the query, so `?page=2` does not re-render the frame. */
const usePathname = () =>
  useLocation({ select: (location) => location.pathname });

/** The page the URL is in. Overview only on its exact path. */
function activePage(pathname: string, base: string): NavPage | undefined {
  return PROJECT_PAGES.find((page) =>
    page.href === ''
      ? pathname === base || pathname === `${base}/`
      : isUnder(pathname, `${base}${page.href}`),
  );
}

/** The project the URL is in, as the project layout's loader cached it. */
function useCurrentProject(): FrameProject | null {
  const t = useTranslations('projects');
  const match = useMatch({
    from: '/_authenticated/projects/$id',
    shouldThrow: false,
  });
  const id = match ? match.params.id : null;
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
  const tCommon = useTranslations('common');
  const pathname = usePathname();
  const router = useRouter();
  const project = useCurrentProject();
  const logout = useMutation(userMutations.logout());
  const signingOut = logout.isPending;

  // One read for the switcher and the command bar; creating or deleting a
  // project invalidates it. A failed read still renders both: the static
  // commands are the bulk of the bar, and an empty switcher still leads to
  // "All projects".
  const read = useQuery(projectQueries.picker());
  const projects = read.data?.items ?? [];

  const base = project ? `/projects/${project.id}` : '';
  const { settingsMode, scope, depth } = sidebarScope(pathname, project);
  const permissions = usePermissionsLater();
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
    ...(permissions.canCreateProject
      ? [
          {
            id: 'new',
            label: t('switcher.new'),
            icon: NewIcon,
            separated: true,
            onSelect: () => router.push('/projects/new'),
          },
        ]
      : []),
  ];

  const signOut = () =>
    logout.mutate(undefined, { onSettled: () => router.push('/login') });

  return (
    <Sidebar
      aria-label={tCommon('mainNavigation')}
      header={
        <>
          {/* Inside a project the top row is its switcher; above them all
              there is nothing to switch, and the row is the mark. */}
          <SidebarBrand
            collapseLabel={tCommon('collapseSidebar')}
            expandLabel={tCommon('expandSidebar')}
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
            projects={projects}
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
            label={tCommon('account', { name: user.email })}
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
            permissions={permissions}
          />
        ) : project ? (
          <ProjectNav project={project} permissions={permissions} />
        ) : (
          <WorkspaceNav projects={projects} permissions={permissions} />
        )}
      </SidebarScope>
    </Sidebar>
  );
}

/** Every settings page: the account, the workspace, and each project's. */
function SettingsNav({
  project,
  projects,
  permissions,
}: {
  project: FrameProject | null;
  projects: ProjectOption[];
  permissions: Permissions;
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
        {WORKSPACE_SETTINGS.filter((e) => allowed(e, permissions)).map(
          (entry) => (
            <SettingsLink key={entry.key} entry={entry} pathname={pathname} />
          ),
        )}
      </SidebarGroup>
      {listed.length > 0 ? (
        <SidebarGroup label={t('switcher.projects')}>
          {listed.map((option) => {
            const optionBase = `/projects/${option.id}`;
            return (
              <SidebarSubmenu
                key={option.id}
                label={option.name}
                leading={<ProjectLogo project={option} size={18} />}
                defaultOpen={option.id === project?.id}
              >
                {PROJECT_SETTINGS.filter((e) =>
                  allowed(e, permissions, option.id),
                ).map((entry) => (
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

/** A project's pages, in its own order, without the products it turned off. */
function ProjectNav({
  project,
  permissions,
}: {
  project: FrameProject;
  permissions: Permissions;
}) {
  const t = useTranslations('projects');
  const pathname = usePathname();
  const base = `/projects/${project.id}`;
  const active = activePage(pathname, base);
  const shown = (page: NavPage) =>
    allowed(page, permissions, project.id) &&
    (!page.product || keepsProduct(project.disabled_products, page.product));
  return (
    <>
      <SidebarItem
        icon={OVERVIEW.icon}
        label={t('nav.overview')}
        active={active === OVERVIEW}
        render={<Link href={base} />}
      />
      {PROJECT_GROUPS.filter((group) => group.pages.some(shown)).map(
        (group) => (
          <SidebarGroup key={group.label} label={t(`nav.${group.label}`)}>
            {inProductOrder(
              group.pages.filter(shown),
              project.product_order,
            ).map((page) => (
              <SidebarItem
                key={page.key}
                icon={page.icon}
                label={t(`nav.${page.key}` as 'nav.issues')}
                active={active === page}
                render={<Link href={`${base}${page.href}`} />}
              />
            ))}
          </SidebarGroup>
        ),
      )}
    </>
  );
}

/** Above any one project: the list of them, the team, and the projects. */
function WorkspaceNav({
  projects,
  permissions,
}: {
  projects: ProjectOption[];
  permissions: Permissions;
}) {
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
      {canSeeTeam(permissions) ? (
        <SidebarItem
          icon={TeamIcon}
          label={tSettings('nav.team')}
          active={isUnder(pathname, '/settings/team')}
          render={<Link href="/settings/team" />}
        />
      ) : null}
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
  entry: NavPage;
  pathname: string;
}) {
  const tSettings = useTranslations('settings');
  return (
    <SidebarItem
      icon={entry.icon}
      label={tSettings(`nav.${entry.key}` as 'nav.account')}
      // A page with pages under it -- an integration's own -- stays marked.
      active={isUnder(pathname, entry.href)}
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
  page: NavPage | undefined,
  label: string,
  trail: TrailCrumb[],
): Crumb[] {
  const section = page?.href ? `${base}${page.href}` : null;
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
  const tCommon = useTranslations('common');
  const pathname = usePathname();
  const project = useCurrentProject();
  const trail = usePageTrailCrumbs();

  let items: Crumb[];

  const base = project ? `/projects/${project.id}` : '';
  // The settings page the URL is on, for the trail's last crumb.
  const settingsPage = [
    ...ACCOUNT_SETTINGS,
    ...WORKSPACE_SETTINGS,
    ...PROJECT_SETTINGS.map((e) => ({ ...e, href: `${base}${e.href}` })),
  ].find((e) => isUnder(pathname, e.href));
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
    const page = activePage(pathname, base);
    items = [
      ...projectCrumb,
      ...sectionCrumbs(
        pathname,
        base,
        page,
        page
          ? t(`nav.${page.key}` as 'nav.issues')
          : tSettings('nav.projectSettingsTitle'),
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

  return <Breadcrumbs items={items} label={tCommon('breadcrumb')} />;
}
