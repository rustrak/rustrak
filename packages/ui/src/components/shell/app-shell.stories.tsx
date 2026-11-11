import type { Meta, StoryObj } from '@storybook/react-vite';
import { useState } from 'react';
import { expect, userEvent, within } from 'storybook/test';
import { Avatar } from '../avatar/avatar';
import { Breadcrumbs } from '../breadcrumbs/breadcrumbs';
import { Button } from '../button/button';
import { SplitButton } from '../button/split-button';
import { Count } from '../count/count';
import {
  AgentsIcon,
  AssignIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  CronsIcon,
  DocsIcon,
  ExportIcon,
  FeedbackIcon,
  IssuesIcon,
  LogsIcon,
  MuteIcon,
  NewIcon,
  NotificationIcon,
  OverflowIcon,
  OverviewIcon,
  PerformanceIcon,
  ProjectsIcon,
  ReleasesIcon,
  ResolveIcon,
  SaveViewIcon,
  SettingsIcon,
  SignOutIcon,
  TeamIcon,
} from '../icon/icon-catalog';
import { Menu } from '../menu/menu';
import {
  SegmentedControl,
  SegmentedItem,
} from '../segmented-control/segmented-control';
import { Tab, TabList, TabPanel, Tabs } from '../tabs/tabs';
import { Tag } from '../tag/tag';
import { Text } from '../text/text';
import { AppShell, Page, PageHeader } from './app-shell';
import {
  Sidebar,
  SidebarBrand,
  SidebarGroup,
  SidebarItem,
  SidebarProject,
  SidebarScope,
  SidebarSearch,
  SidebarUser,
} from './sidebar';
import { Topbar, TopbarMenuButton } from './topbar';

/*
 * These are composition stories rather than props stories: what they check is
 * that the frame, the sidebar, the tabs and the buttons still add up to the
 * screens of the product. `Meta` is left ungenericised on purpose -- typing it
 * to `AppShell` would demand `args` on every story, and none of them has an
 * argument to give.
 */
const meta = {
  title: 'Shell/AppShell',
  parameters: { layout: 'fullscreen', controls: { disable: true } },
} satisfies Meta;

export default meta;
type Story = StoryObj<typeof meta>;

const GROUPS = [
  {
    label: 'Triage',
    routes: [
      { icon: IssuesIcon, label: 'Issues', count: '142' },
      { icon: FeedbackIcon, label: 'Feedback', status: 'info' },
      { icon: ReleasesIcon, label: 'Releases' },
    ],
  },
  {
    label: 'Monitor',
    routes: [
      { icon: PerformanceIcon, label: 'Performance' },
      { icon: LogsIcon, label: 'Logs' },
      { icon: CronsIcon, label: 'Crons', status: 'warning' },
      { icon: NotificationIcon, label: 'Alerts' },
      { icon: AgentsIcon, label: 'Agents' },
    ],
  },
] as const;

function ShellSidebar({ active }: { active: string }) {
  return (
    <Sidebar
      aria-label="Main navigation"
      header={
        <>
          <SidebarBrand
            collapseLabel="Collapse sidebar"
            expandLabel="Expand sidebar"
            render={<a href="#home" />}
            switcher={
              <SidebarProject
                name="storefront-web"
                detail="JavaScript"
                platform="JS"
              />
            }
          />
          <SidebarSearch />
        </>
      }
      footer={
        <>
          <SidebarItem
            icon={SettingsIcon}
            label="Settings"
            render={<a href="#settings" />}
          />
          <SidebarItem
            icon={DocsIcon}
            label="Docs"
            render={<a href="#docs" />}
          />
          <SidebarUser
            label="Account: Mara Kovač"
            name="Mara Kovač"
            detail="Admin"
            email="mara@acme.dev"
            version="rustrak 0.21.1"
            actions={[
              { id: 'account', label: 'Account' },
              {
                id: 'signout',
                label: 'Sign out',
                icon: SignOutIcon,
                tone: 'danger',
                separated: true,
              },
            ]}
          />
        </>
      }
    >
      <SidebarItem
        icon={OverviewIcon}
        label="Overview"
        active={active === 'Overview'}
        render={<a href="#overview" />}
      />
      {GROUPS.map((group) => (
        <SidebarGroup key={group.label} label={group.label}>
          {group.routes.map((route) => (
            <SidebarItem
              key={route.label}
              icon={route.icon}
              label={route.label}
              count={'count' in route ? route.count : undefined}
              status={'status' in route ? route.status : undefined}
              active={route.label === active}
              render={<a href={`#${route.label.toLowerCase()}`} />}
            />
          ))}
        </SidebarGroup>
      ))}
    </Sidebar>
  );
}

function Frame({
  active = 'Issues',
  defaultCollapsed,
  crumbs = [{ label: 'Issues' }],
  actions,
  children,
}: {
  actions?: React.ReactNode;
  active?: string;
  defaultCollapsed?: boolean;
  crumbs?: { label: string; mono?: boolean }[];
  children: React.ReactNode;
}) {
  return (
    <AppShell
      defaultCollapsed={defaultCollapsed}
      sidebar={<ShellSidebar active={active} />}
    >
      <Topbar
        menu={
          <TopbarMenuButton
            openLabel="Open navigation"
            closeLabel="Close navigation"
          />
        }
        actions={actions}
      >
        <Breadcrumbs
          items={[
            {
              label: 'storefront-web',
              icon: (
                <Avatar
                  shape="square"
                  size="xs"
                  tone="brand"
                  name="storefront-web"
                  initials="JS"
                />
              ),
              render: <a href="#project" />,
            },
            ...crumbs,
          ]}
        />
      </Topbar>
      {children}
    </AppShell>
  );
}

const ISSUES = [
  {
    tone: 'error',
    level: 'error',
    title: "TypeError: Cannot read properties of undefined (reading 'total')",
    tag: 'regression',
    where: 'src/checkout/summary.tsx · renderTotals · CHECKOUT-API-4F2',
    events: '12.431',
    seen: '3 min ago',
  },
  {
    tone: 'error',
    level: 'fatal',
    title: 'ConnectionTimeout: pool exhausted after 30000ms',
    where: 'db/pool.rs · acquire · CHECKOUT-API-3B1',
    events: '3.902',
    seen: '12 min ago',
  },
  {
    tone: 'warning',
    level: 'warn',
    title: 'ValidationError: coupon code not applicable',
    tag: 'new',
    where: 'src/promo/apply.ts · applyCoupon · CHECKOUT-API-2C8',
    events: '1.204',
    seen: '38 min ago',
  },
  {
    tone: 'info',
    level: 'info',
    title: 'DeprecationWarning: legacy shipping API called',
    where: 'src/shipping/legacy.ts · quote · CHECKOUT-API-1D5',
    events: '288',
    seen: '5 h ago',
  },
] as const;

function IssueList() {
  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-border-subtle bg-panel">
      <div className="flex h-row-head shrink-0 items-center gap-4 border-b border-border-subtle bg-surface px-4">
        <Text variant="column" tone="meta" className="w-16 shrink-0">
          Level
        </Text>
        <Text variant="column" tone="meta" className="flex-1">
          Issue
        </Text>
        <Text variant="column" tone="meta" className="w-20 shrink-0 text-end">
          Events
        </Text>
        <Text variant="column" tone="meta" className="w-24 shrink-0 text-end">
          Last seen
        </Text>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {ISSUES.map((issue) => (
          <div
            key={issue.title}
            className="flex h-row items-center gap-4 border-b border-border-divider px-4 hover:bg-surface-hover"
          >
            <span className="w-16 shrink-0">
              <Tag tone={issue.tone}>{issue.level}</Tag>
            </span>
            <span className="flex min-w-0 flex-1 flex-col gap-0.5">
              <span className="flex min-w-0 items-center gap-2.5">
                <Text variant="value" truncate className="font-medium">
                  {issue.title}
                </Text>
                {'tag' in issue && issue.tag ? (
                  <Tag tone={issue.tag === 'new' ? 'brand' : 'warning'}>
                    {issue.tag}
                  </Tag>
                ) : null}
              </span>
              <Text variant="mono-sm" tone="ghost" truncate>
                {issue.where}
              </Text>
            </span>
            <span className="w-20 shrink-0 text-end">
              <Count tone="strong">{issue.events}</Count>
            </span>
            <span className="w-24 shrink-0 text-end">
              <Count>{issue.seen}</Count>
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

/**
 * The list screen. Three bands and no more: the title with its facts and
 * every control for the page on the same line, the tabs on the header's
 * bottom rule, and the rows in their card.
 */
export const IssueListScreen: Story = {
  render: () => (
    <Frame
      actions={
        <Button variant="ghost" icon={SaveViewIcon}>
          Save view
        </Button>
      }
    >
      <Tabs defaultValue="unresolved" className="flex min-h-0 flex-1 flex-col">
        <PageHeader
          title="Issues"
          summary={['142 unresolved', '3 regressions', '12.4K events today']}
          actions={
            <>
              <SegmentedControl defaultValue="24 h" aria-label="Time range">
                {['1 h', '24 h', '7 d', '30 d'].map((range) => (
                  <SegmentedItem key={range} value={range}>
                    {range}
                  </SegmentedItem>
                ))}
              </SegmentedControl>
              <Button variant="secondary" icon={ExportIcon}>
                Export
              </Button>
              <Button variant="primary" icon={NewIcon}>
                New alert
              </Button>
            </>
          }
          tabs={
            <TabList
              meta={
                <Text variant="meta" tone="meta">
                  Sorted by events
                </Text>
              }
            >
              <Tab value="unresolved">
                Unresolved
                <Count>143</Count>
              </Tab>
              <Tab value="resolved">
                Resolved
                <Count>1.208</Count>
              </Tab>
              <Tab value="muted">
                Muted
                <Count>36</Count>
              </Tab>
            </TabList>
          }
        />
        <TabPanel
          value="unresolved"
          className="flex min-h-0 flex-1 px-page-gutter py-5"
        >
          <IssueList />
        </TabPanel>
        <TabPanel value="resolved" />
        <TabPanel value="muted" />
      </Tabs>
    </Frame>
  ),
};

/** The record screen: record paging in the topbar, the primary action, the record's tabs. */
export const IssueDetailScreen: Story = {
  render: () => (
    <Frame
      crumbs={[{ label: 'Issues' }, { label: 'STOREFRONT-3K', mono: true }]}
      actions={
        <>
          <Count>12 of 143</Count>
          <Button
            size="xs"
            variant="secondary"
            icon={ChevronLeftIcon}
            aria-label="Previous issue"
          />
          <Button
            size="xs"
            variant="secondary"
            icon={ChevronRightIcon}
            aria-label="Next issue"
          />
        </>
      }
    >
      <Page scroll={false}>
        <div className="flex shrink-0 items-start justify-between gap-6">
          <div className="min-w-0">
            <div className="mb-2 flex items-center gap-2.5">
              <Tag tone="error">error</Tag>
              <Tag tone="warning">regression</Tag>
              <Text variant="mono-sm" tone="ghost">
                unhandled · production · eu-west-1
              </Text>
            </div>
            <h1 className="flex min-w-0 items-baseline gap-2.5 text-title text-fg">
              TypeError
              <Text variant="mono" tone="muted" truncate>
                Cannot read properties of undefined (reading 'total')
              </Text>
            </h1>
          </div>

          <div className="flex shrink-0 items-center gap-2">
            <SplitButton
              icon={ResolveIcon}
              menuLabel="More ways to resolve"
              actions={[
                { id: 'next', label: 'Resolve in the next release' },
                { id: 'again', label: 'Resolve until it happens again' },
                { id: 'assign', label: 'Assign and resolve', icon: AssignIcon },
              ]}
            >
              Resolve
            </SplitButton>
            <Button variant="secondary" icon={MuteIcon}>
              Mute
            </Button>
            <Button
              variant="ghost"
              icon={OverflowIcon}
              aria-label="More actions"
            />
          </div>
        </div>

        <Tabs defaultValue="details" className="min-h-0 flex-1">
          <TabList>
            <Tab size="sm" value="details">
              Details
            </Tab>
            <Tab size="sm" value="events">
              Events
              <Count>12,4 K</Count>
            </Tab>
            <Tab size="sm" value="breadcrumbs">
              Breadcrumbs
            </Tab>
            <Tab size="sm" value="traces">
              Traces
            </Tab>
          </TabList>
          <TabPanel value="details">
            <Text tone="muted">The stack, the tags, the release.</Text>
          </TabPanel>
          <TabPanel value="events" />
          <TabPanel value="breadcrumbs" />
          <TabPanel value="traces" />
        </Tabs>
      </Page>
    </Frame>
  ),
};

/** The rail. Collapsed, every route keeps its name in a tooltip. */
export const CollapsedSidebar: Story = {
  render: () => (
    <Frame active="Overview" defaultCollapsed crumbs={[{ label: 'Overview' }]}>
      <PageHeader title="Overview" />
    </Frame>
  ),
};

/** ⌘B collapses it, and the button says which state it is in. */
export const TheShortcutCollapsesIt: Story = {
  render: () => (
    <Frame active="Overview" crumbs={[{ label: 'Overview' }]}>
      <PageHeader title="Overview" />
    </Frame>
  ),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const collapse = canvas.getByRole('button', { name: 'Collapse sidebar' });

    await expect(collapse).toHaveAttribute('aria-expanded', 'true');

    await userEvent.keyboard('{Meta>}b{/Meta}');

    await expect(
      await canvas.findByRole('button', { name: 'Expand sidebar' }),
    ).toHaveAttribute('aria-expanded', 'false');
  },
};

/** The route you are on says so to a screen reader, not only in lime. */
export const TheActiveRouteIsAnnounced: Story = {
  render: () => (
    <Frame>
      <PageHeader title="Issues" />
    </Frame>
  ),
  play: async ({ canvasElement }) => {
    const nav = within(canvasElement).getByRole('navigation', {
      name: 'Main navigation',
    });

    await expect(
      within(nav).getByRole('link', { name: /Issues/ }),
    ).toHaveAttribute('aria-current', 'page');
  },
};

/** Nothing in the frame scrolls except the content. */
export const OnlyTheContentScrolls: Story = {
  render: () => (
    <Frame active="Logs" crumbs={[{ label: 'Logs' }]}>
      <PageHeader title="Logs" />
      <Page>
        {Array.from({ length: 60 }, (_, index) => (
          <Text key={index} variant="mono-sm" tone="muted">
            2026-08-23 11:0{index % 10}:37 · checkout-api · request completed
          </Text>
        ))}
      </Page>
    </Frame>
  ),
};

const PROJECTS = ['storefront-web', 'billing-api', 'mobile-ios'];

/** Into a project and back out, through the scope card. */
function ScopeSidebar() {
  const [project, setProject] = useState<string | null>(null);

  return (
    <AppShell
      sidebar={
        <Sidebar
          aria-label="Main navigation"
          header={
            <>
              <SidebarBrand
                collapseLabel="Collapse sidebar"
                expandLabel="Expand sidebar"
                render={<a href="#home" />}
              />
              <Menu
                trigger={
                  <SidebarProject
                    name={project ?? 'All projects'}
                    detail={project ?? '3 projects'}
                    platform="JS"
                  />
                }
                actions={[
                  {
                    id: 'all',
                    label: 'All projects',
                    icon: ProjectsIcon,
                    onSelect: () => setProject(null),
                  },
                  ...PROJECTS.map((name, index) => ({
                    id: name,
                    label: name,
                    selected: name === project,
                    separated: index === 0,
                    onSelect: () => setProject(name),
                  })),
                ]}
              />
              <SidebarSearch />
            </>
          }
        >
          <SidebarScope
            scope={project ? `project:${project}` : 'workspace'}
            depth={project ? 1 : 0}
          >
            {project ? (
              <>
                <SidebarItem
                  icon={OverviewIcon}
                  label="Overview"
                  href="#overview"
                  active
                />
                {GROUPS.map((group) => (
                  <SidebarGroup key={group.label} label={group.label}>
                    {group.routes.map((route) => (
                      <SidebarItem
                        key={route.label}
                        icon={route.icon}
                        label={route.label}
                      />
                    ))}
                  </SidebarGroup>
                ))}
              </>
            ) : (
              <>
                <SidebarItem icon={ProjectsIcon} label="Projects" active />
                <SidebarItem icon={TeamIcon} label="Team" href="#team" />
                <SidebarGroup label="Projects">
                  {PROJECTS.map((name) => (
                    <SidebarItem
                      key={name}
                      leading={<Avatar shape="square" size="xs" name={name} />}
                      label={name}
                      href={`#${name}`}
                    />
                  ))}
                </SidebarGroup>
              </>
            )}
          </SidebarScope>
        </Sidebar>
      }
    >
      <Topbar>
        <Breadcrumbs items={[{ label: project ?? 'Projects' }]} />
      </Topbar>
    </AppShell>
  );
}

/** Going in slides forward, coming back slides back; the first paint is still. */
export const ChangesScope: Story = {
  render: () => <ScopeSidebar />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(document.body);
    const nav = canvas.getByRole('navigation', { name: 'Main navigation' });
    const scope = () => nav.querySelector('[data-direction], .group\\/scope');

    await expect(scope()).not.toHaveAttribute('data-direction');

    await userEvent.click(canvas.getByRole('button', { name: /All projects/ }));
    await userEvent.click(
      await page.findByRole('menuitem', { name: /billing-api/ }),
    );
    await expect(
      await canvas.findByRole('link', { name: /Overview/ }),
    ).toBeInTheDocument();
    await expect(scope()).toHaveAttribute('data-direction', 'forward');

    await userEvent.click(canvas.getByRole('button', { name: /billing-api/ }));
    await userEvent.click(
      await page.findByRole('menuitem', { name: /All projects/ }),
    );
    await expect(
      await canvas.findByRole('link', { name: /Team/ }),
    ).toBeInTheDocument();
    await expect(scope()).toHaveAttribute('data-direction', 'back');
  },
};
