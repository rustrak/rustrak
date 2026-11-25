import type { UserResponse } from '@rustrak/client';
import {
  Avatar,
  BackIcon,
  ChevronDownIcon,
  ChevronLeftIcon,
  DocsIcon,
  ExternalLinkIcon,
  IssuesIcon,
  MenuIcon,
  NewIcon,
  ProjectsIcon,
  SearchIcon,
  SettingsIcon,
  Sheet,
  SheetRow,
  SheetSection,
  SignOutIcon,
  TabBar,
  TabBarItem,
  TeamIcon,
  Wordmark,
} from '@rustrak/ui';
import { useMutation, useQuery } from '@tanstack/react-query';
import {
  type ComponentProps,
  createContext,
  type ReactNode,
  use,
  useMemo,
  useState,
} from 'react';
import { useTranslations } from 'use-intl';
import { pluginIcon } from '@/features/plugin/model/icon';
import {
  activeEntry,
  usePluginNav,
} from '@/features/plugin/ui/hooks/use-plugin-nav';
import { projectQueries } from '@/features/project/api/queries';
import { userMutations } from '@/features/user/api/mutations';
import type { Permissions } from '@/shared/lib/permissions';
import { Link } from '@/shared/ui/components/link';
import { promptInstall, useInstallMode } from '@/shared/ui/hooks/use-install';
import { usePermissionsLater } from '@/shared/ui/hooks/use-permissions';
import { useRouter } from '@/shared/ui/hooks/use-router';
import {
  type FrameProject,
  ProjectLogo,
  useCurrentProject,
  usePathname,
} from './app-frame';
import { openCommandBar } from './command-bar/open-command-bar';
import {
  ACCOUNT_SETTINGS,
  activePage,
  allowed,
  canSeeTeam,
  DOCS_URL,
  inProductOrder,
  isSettingsPath,
  isUnder,
  type NavPage,
  OVERVIEW,
  PROJECT_GROUPS,
  PROJECT_SETTINGS,
  shownInProject,
  workspaceGroups,
} from './nav';

/**
 * The phone's frame: the tab bar at the bottom, the "More" sheet behind its
 * last tab, and the heading at the top of the sheet. Below `md` it replaces
 * the sidebar; it reads the same tables (`nav.ts`), so a page reachable from
 * the sidebar is reachable from here, under the same name.
 *
 * Why tabs and not the sidebar as a drawer: a drawer hides every destination
 * behind a button in the corner furthest from the thumb, and what is out of
 * sight is used less (Nielsen Norman Group measured it: hidden navigation is
 * discovered roughly half as often). The three places someone opens an error
 * tracker on a phone for -- the project's state, its issues, and one monitor
 * -- are one tap away instead, with search beside them, and everything else
 * is one tap further, in "More".
 */

type SheetView = 'root' | 'projects' | 'settings' | 'install';

interface MobileNavState {
  open: (view?: SheetView) => void;
}

const MobileNavContext = createContext<MobileNavState>({ open: () => {} });

/**
 * Owns the "More" sheet, so the tab bar and the heading can both open it: the
 * tab at its root, the project chip at the project list, the settings chip at
 * the settings pages.
 */
export function MobileNav({
  user,
  children,
}: {
  user: UserResponse;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const [view, setView] = useState<SheetView>('root');
  const state = useMemo<MobileNavState>(
    () => ({
      open: (next = 'root') => {
        setView(next);
        setOpen(true);
      },
    }),
    [],
  );

  return (
    <MobileNavContext value={state}>
      {children}
      <MoreSheet
        user={user}
        open={open}
        onOpenChange={setOpen}
        view={view}
        onViewChange={setView}
      />
    </MobileNavContext>
  );
}

const useMobileNav = () => use(MobileNavContext);

/** A project's tabs: overview, then the first page of each sidebar group. */
function projectTabs(
  project: FrameProject,
  permissions: Permissions,
): NavPage[] {
  const shown = (page: NavPage) => shownInProject(page, permissions, project);
  const firsts = PROJECT_GROUPS.map(
    (group) =>
      inProductOrder(group.pages.filter(shown), project.product_order)[0],
  ).filter((page): page is NavPage => page !== undefined);
  return [...(shown(OVERVIEW) ? [OVERVIEW] : []), ...firsts];
}

export function AppTabBar() {
  const t = useTranslations('projects');
  const tMobile = useTranslations('mobile');
  const pathname = usePathname();
  const project = useCurrentProject();
  const permissions = usePermissionsLater();
  const { open } = useMobileNav();

  let tabs: { key: string; node: ReactNode; active: boolean }[];

  if (project) {
    const base = `/projects/${project.id}`;
    const current = isSettingsPath(pathname)
      ? undefined
      : activePage(pathname, base);
    tabs = projectTabs(project, permissions).map((page) => ({
      key: page.key,
      active: current === page,
      node: (
        <TabBarItem
          key={page.key}
          icon={page.icon}
          label={t(`nav.${page.key}` as 'nav.issues')}
          active={current === page}
          render={
            <Link
              href={`${base}${page.href}`}
              activeOptions={{ exact: true }}
            />
          }
        />
      ),
    }));
  } else {
    const onProjects = pathname === '/projects' || pathname === '/projects/new';
    const onIssues = isUnder(pathname, '/issues');
    tabs = [
      {
        key: 'projects',
        active: onProjects,
        node: (
          <TabBarItem
            key="projects"
            icon={ProjectsIcon}
            label={tMobile('projects')}
            active={onProjects}
            render={<Link href="/projects" activeOptions={{ exact: true }} />}
          />
        ),
      },
      ...(permissions.canAnywhere('issues', 'read')
        ? [
            {
              key: 'issues',
              active: onIssues,
              node: (
                <TabBarItem
                  key="issues"
                  icon={IssuesIcon}
                  label={t('nav.issues')}
                  active={onIssues}
                  render={<Link href="/issues" />}
                />
              ),
            },
          ]
        : []),
    ];
  }

  // "More" is where the page is when no tab claims it: Logs, a settings page.
  const elsewhere = !tabs.some((tab) => tab.active);

  return (
    <TabBar aria-label={tMobile('tabs')}>
      {tabs.map((tab) => tab.node)}
      <TabBarItem
        icon={SearchIcon}
        label={tMobile('search')}
        render={<button type="button" onClick={openCommandBar} />}
      />
      <TabBarItem
        icon={MenuIcon}
        label={tMobile('more')}
        active={elsewhere}
        render={<button type="button" onClick={() => open()} />}
      />
    </TabBar>
  );
}

/**
 * The top of the sheet on a phone, where the desktop draws the breadcrumbs.
 *
 * A trail of four crumbs does not fit across 390 px, and it does not need to:
 * on a phone the question the top bar answers is "where does Back go". So it
 * is one of two things.
 *
 * - **Below a section** (an issue, a release, a settings page's own page),
 *   a back button named after where it goes: iOS's "‹ Issues".
 * - **At a section's root**, the scope the page is in, as a button that
 *   changes it: the project (opens the project list), Settings (opens the
 *   settings pages), or the mark when there is neither.
 */
export function MobileHeading({
  back,
}: {
  back?: { label: string; href: string };
}) {
  const tMobile = useTranslations('mobile');
  const tSettings = useTranslations('settings');
  const pathname = usePathname();
  const project = useCurrentProject();
  const { open } = useMobileNav();

  if (back) {
    return (
      <Link
        href={back.href}
        className="-ms-1.5 flex min-h-control-sm min-w-0 items-center gap-0.5 rounded-lg pe-2 text-value text-fg-secondary active:text-fg"
      >
        <ChevronLeftIcon size="3xl" className="shrink-0 text-fg-brand" />
        <span className="truncate">{back.label}</span>
      </Link>
    );
  }

  if (isSettingsPath(pathname)) {
    return (
      <ScopeChip onClick={() => open('settings')}>
        <SettingsIcon size="2xl" className="shrink-0 text-fg-subtle" />
        <span className="truncate">{tSettings('nav.settingsTitle')}</span>
      </ScopeChip>
    );
  }

  if (project) {
    return (
      <ScopeChip
        onClick={() => open('projects')}
        aria-label={tMobile('switchProject', { name: project.name })}
      >
        <ProjectLogo project={project} size={18} />
        <span className="truncate">{project.name}</span>
      </ScopeChip>
    );
  }

  return (
    <Link href="/projects" aria-label="Rustrak" className="flex items-center">
      <Wordmark still className="h-wordmark w-auto text-fg" />
    </Link>
  );
}

function ScopeChip({
  children,
  ...props
}: { children: ReactNode } & ComponentProps<'button'>) {
  return (
    <button
      type="button"
      className="-ms-1.5 flex min-h-control-sm min-w-0 items-center gap-2 rounded-lg px-1.5 text-value font-medium text-fg active:bg-surface-hover"
      {...props}
    >
      {children}
      <ChevronDownIcon size="lg" className="shrink-0 text-fg-subtle" />
    </button>
  );
}

function MoreSheet({
  user,
  open,
  onOpenChange,
  view,
  onViewChange,
}: {
  user: UserResponse;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  view: SheetView;
  onViewChange: (view: SheetView) => void;
}) {
  const tMobile = useTranslations('mobile');
  const tCommon = useTranslations('common');
  const tSettings = useTranslations('settings');
  const titles: Record<SheetView, string> = {
    root: tMobile('more'),
    projects: tMobile('projects'),
    settings: tSettings('nav.settingsTitle'),
    install: tMobile('install'),
  };
  const close = () => onOpenChange(false);

  return (
    <Sheet
      open={open}
      onOpenChange={onOpenChange}
      title={titles[view]}
      closeLabel={tCommon('close')}
      leading={
        view === 'root' ? undefined : (
          <button
            type="button"
            aria-label={tMobile('back')}
            onClick={() => onViewChange('root')}
            className="-ms-1 flex size-control-sm shrink-0 items-center justify-center rounded-pill text-fg-brand active:bg-surface-hover"
          >
            <BackIcon size="2xl" />
          </button>
        )
      }
    >
      {/* Keyed by view so each one enters as a page would: from the right
          going in, with the list scrolled to its top. */}
      <div key={view} className="animate-enter-forward">
        {view === 'root' ? (
          <RootView user={user} onNavigate={close} onView={onViewChange} />
        ) : view === 'projects' ? (
          <ProjectsView onNavigate={close} />
        ) : view === 'settings' ? (
          <SettingsView onNavigate={close} />
        ) : (
          <InstallSteps />
        )}
      </div>
    </Sheet>
  );
}

function RootView({
  user,
  onNavigate,
  onView,
}: {
  user: UserResponse;
  onNavigate: () => void;
  onView: (view: SheetView) => void;
}) {
  const t = useTranslations('projects');
  const tMobile = useTranslations('mobile');
  const tSettings = useTranslations('settings');
  const tUser = useTranslations('user');
  const pathname = usePathname();
  const router = useRouter();
  const project = useCurrentProject();
  const permissions = usePermissionsLater();
  const install = useInstallMode();
  // On the mutation, not on `mutate`: a per-call callback is dropped if this
  // view unmounts first (the reader opened Projects mid sign-out).
  const logout = useMutation({
    ...userMutations.logout(),
    onSettled: () => {
      onNavigate();
      router.push('/login');
    },
  });

  const base = project ? `/projects/${project.id}` : '';
  const inTabs = project
    ? new Set(projectTabs(project, permissions).map((page) => page.key))
    : new Set<string>();
  const shown = (page: NavPage) =>
    project ? shownInProject(page, permissions, project) : false;
  const current = project ? activePage(pathname, base) : undefined;
  const pluginGroups = usePluginNav(project?.id);

  const link = (href: string) => (
    <Link href={href} onClick={onNavigate} activeOptions={{ exact: true }} />
  );

  return (
    <>
      <SheetSection>
        <SheetRow
          leading={
            project ? (
              <ProjectLogo project={project} size={26} />
            ) : (
              <ProjectsIcon size="2xl" />
            )
          }
          label={project ? project.name : tMobile('chooseProject')}
          detail={project ? tMobile('switchProjectHint') : undefined}
          chevron
          onClick={() => onView('projects')}
        />
      </SheetSection>

      {project
        ? PROJECT_GROUPS.map((group) => {
            const rest = inProductOrder(
              group.pages.filter(shown),
              project.product_order,
            ).filter((page) => !inTabs.has(page.key));
            return rest.length > 0 ? (
              <SheetSection key={group.label} label={t(`nav.${group.label}`)}>
                {rest.map((page) => (
                  <SheetRow
                    key={page.key}
                    icon={page.icon}
                    label={t(`nav.${page.key}` as 'nav.issues')}
                    active={current === page}
                    render={link(`${base}${page.href}`)}
                  />
                ))}
              </SheetSection>
            ) : null;
          })
        : null}

      {pluginGroups.map((group) => {
        const active = activeEntry(group.entries, pathname);
        return (
          <SheetSection key={group.id} label={group.name}>
            {group.entries.map((page) => (
              <SheetRow
                key={page.key}
                icon={pluginIcon(page.icon)}
                label={page.label}
                active={page.key === active}
                render={link(page.href)}
              />
            ))}
          </SheetSection>
        );
      })}

      <SheetSection label={tMobile('workspace')}>
        {project ? (
          <SheetRow
            icon={ProjectsIcon}
            label={t('switcher.all')}
            render={link('/projects')}
          />
        ) : null}
        {project && permissions.canAnywhere('issues', 'read') ? (
          <SheetRow
            icon={IssuesIcon}
            label={tMobile('allIssues')}
            render={link('/issues')}
          />
        ) : null}
        {canSeeTeam(permissions) ? (
          <SheetRow
            icon={TeamIcon}
            label={tSettings('nav.team')}
            active={isUnder(pathname, '/settings/team')}
            render={link('/settings/team')}
          />
        ) : null}
        <SheetRow
          icon={SettingsIcon}
          label={tSettings('nav.settingsTitle')}
          chevron
          onClick={() => onView('settings')}
        />
      </SheetSection>

      <SheetSection>
        {install === 'prompt' || install === 'manual' ? (
          <SheetRow
            icon={NewIcon}
            label={tMobile('install')}
            detail={tMobile('installHint')}
            chevron={install === 'manual'}
            onClick={() =>
              install === 'prompt' ? void promptInstall() : onView('install')
            }
          />
        ) : null}
        <SheetRow
          icon={DocsIcon}
          label={t('nav.docs')}
          trailing={
            <ExternalLinkIcon size="lg" className="shrink-0 text-fg-ghost" />
          }
          render={
            <Link href={DOCS_URL} target="_blank" rel="noopener noreferrer" />
          }
        />
      </SheetSection>

      <SheetSection>
        <SheetRow
          leading={<Avatar name={user.email} />}
          label={user.email}
          detail={tUser(`roles.${user.role}`)}
          chevron
          render={link('/settings/profile')}
        />
        <SheetRow
          icon={SignOutIcon}
          danger
          label={
            logout.isPending
              ? tUser('header.signingOut')
              : tUser('header.signOut')
          }
          disabled={logout.isPending}
          onClick={() => logout.mutate()}
        />
      </SheetSection>
    </>
  );
}

function ProjectsView({ onNavigate }: { onNavigate: () => void }) {
  const t = useTranslations('projects');
  const project = useCurrentProject();
  const permissions = usePermissionsLater();
  const projects = useQuery(projectQueries.picker()).data?.items ?? [];

  return (
    <>
      <SheetSection>
        <SheetRow
          icon={ProjectsIcon}
          label={t('switcher.all')}
          render={
            <Link
              href="/projects"
              onClick={onNavigate}
              activeOptions={{ exact: true }}
            />
          }
        />
      </SheetSection>
      {projects.length > 0 ? (
        <SheetSection label={t('switcher.projects')}>
          {projects.map((option) => (
            <SheetRow
              key={option.id}
              leading={<ProjectLogo project={option} size={26} />}
              label={option.name}
              selected={option.id === project?.id}
              render={
                <Link
                  href={`/projects/${option.id}`}
                  onClick={onNavigate}
                  activeOptions={{ exact: true }}
                />
              }
            />
          ))}
        </SheetSection>
      ) : null}
      {permissions.canCreateProject ? (
        <SheetSection>
          <SheetRow
            icon={NewIcon}
            label={t('switcher.new')}
            render={<Link href="/projects/new" onClick={onNavigate} />}
          />
        </SheetSection>
      ) : null}
    </>
  );
}

/** Every settings page, as the sidebar's settings mode lists them. */
function SettingsView({ onNavigate }: { onNavigate: () => void }) {
  const tSettings = useTranslations('settings');
  const pathname = usePathname();
  const project = useCurrentProject();
  const permissions = usePermissionsLater();
  const base = project ? `/projects/${project.id}` : '';

  const row = (entry: NavPage, href: string) => (
    <SheetRow
      key={entry.key}
      icon={entry.icon}
      label={tSettings(`nav.${entry.key}` as 'nav.account')}
      active={isUnder(pathname, href)}
      render={<Link href={href} onClick={onNavigate} />}
    />
  );

  const projectPages = project
    ? PROJECT_SETTINGS.filter((e) => allowed(e, permissions, project.id))
    : [];

  return (
    <>
      {project && projectPages.length > 0 ? (
        <SheetSection label={project.name}>
          {projectPages.map((entry) => row(entry, `${base}${entry.href}`))}
        </SheetSection>
      ) : null}
      <SheetSection label={tSettings('nav.groupAccount')}>
        {ACCOUNT_SETTINGS.map((entry) => row(entry, entry.href))}
      </SheetSection>
      {workspaceGroups(permissions).map((group) => (
        <SheetSection key={group.label} label={tSettings(`nav.${group.label}`)}>
          {group.pages.map((entry) => row(entry, entry.href))}
        </SheetSection>
      ))}
    </>
  );
}

/**
 * Safari hands no install prompt to the page, so on an iPhone or an iPad the
 * button explains the three taps instead. The share glyph is drawn inline:
 * it is Apple's, and the reader is about to look for exactly that shape.
 */
function InstallSteps() {
  const tMobile = useTranslations('mobile');
  const steps = [
    tMobile.rich('installStep1', {
      share: () => (
        <svg
          aria-hidden="true"
          viewBox="0 0 24 24"
          className="mx-0.5 inline size-4 -translate-y-px fill-none stroke-current stroke-2 text-sev-info"
        >
          <path d="M12 3v12M8 7l4-4 4 4M5 11v8a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-8" />
        </svg>
      ),
    }),
    tMobile('installStep2'),
    tMobile('installStep3'),
  ];

  return (
    <>
      <p className="px-1 pt-1 pb-3 text-body text-fg-tertiary">
        {tMobile('installIntro')}
      </p>
      <SheetSection>
        {steps.map((step, index) => (
          <div
            // Fixed, ordered steps: the position is the identity.
            // react-doctor-disable-next-line react-doctor/no-array-index-as-key
            key={index}
            className="flex min-h-menu-item items-center gap-3 px-3 py-2.5 text-value text-fg"
          >
            <span className="flex size-6 shrink-0 items-center justify-center rounded-pill bg-surface-selected text-label text-fg-brand">
              {index + 1}
            </span>
            <span>{step}</span>
          </div>
        ))}
      </SheetSection>
    </>
  );
}
