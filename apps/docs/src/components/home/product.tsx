'use client';

import {
  AgentsIcon,
  AppShell,
  Avatar,
  BarsChart,
  Breadcrumbs,
  Button,
  ChevronDownIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  ClickIcon,
  CollapsePanelIcon,
  CompareIcon,
  ConsoleIcon,
  CronsIcon,
  cn,
  createDataTableColumnHelper,
  DataTable,
  DataTableColumnsButton,
  type DataTableQuery,
  DocsIcon,
  EnvironmentIcon,
  ErrorIcon,
  ExpandPanelIcon,
  FeedbackIcon,
  formatSpanDuration,
  HashIcon,
  IntegrationsIcon,
  IssuesIcon,
  LogsIcon,
  MailIcon,
  MoreIcon,
  MuteIcon,
  NetworkIcon,
  NewIcon,
  NotificationIcon,
  OverviewIcon,
  Page,
  PageHeader,
  PerformanceIcon,
  QueryBar,
  ReleaseIcon,
  ReleasesIcon,
  ResolveIcon,
  SegmentedControl,
  SegmentedItem,
  SettingsIcon,
  Sidebar,
  SidebarBrand,
  SidebarGroup,
  SidebarItem,
  SidebarProject,
  SidebarSearch,
  SidebarUser,
  Sparkline,
  SplitButton,
  StarIcon,
  Switch,
  Tab,
  TabList,
  Tabs,
  Tag,
  type TagTone,
  Text,
  TimeIcon,
  Topbar,
  TrendDownIcon,
  TrendUpIcon,
  UnresolveIcon,
  useDataTable,
  WebhookIcon,
} from '@rustrak/ui';
import { PlatformIcon } from 'platformicons';
import { type ReactNode, useEffect, useRef, useState } from 'react';
import { highlight } from './code';

/**
 * The dashboard's real screens, rebuilt from the same `@rustrak/ui`
 * components and the same English copy as `apps/dashboard`, with fixture data
 * where the queries would be. Each block names the file it mirrors; when that
 * screen changes, this one should follow.
 */

/* ------------------------------------------------------------------------ */
/* Scaling                                                                   */
/** Built once: a formatter is costly to make and every render reuses it. */
const NUMBER = new Intl.NumberFormat('en');
const COMPACT = new Intl.NumberFormat('en', { notation: 'compact' });
const DAY = new Intl.DateTimeFormat('en', { month: 'short', day: 'numeric' });

/* ------------------------------------------------------------------------ */

/**
 * Lays its children out at a desktop width and scales them down to whatever
 * box it is given, so a shot of the product reads like a screenshot instead of
 * a cramped, phone-width version of the app.
 */
export function Scaled({
  width,
  cropped = false,
  children,
  className,
}: {
  width: number;
  /**
   * Lay the UI out on a whole screen and let the box cut it off at the
   * bottom, instead of fitting the screen to the box: a cropped shot of the
   * app, not an app that happens to end where the frame does.
   */
  cropped?: boolean;
  children: ReactNode;
  className?: string;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ scale: 0, height: 0 });

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => {
      const scale = entry.contentRect.width / width;
      const visible = entry.contentRect.height / scale;
      // A desktop screen is at least 16:10, and always taller than what
      // shows, so its bottom edge is never in the shot.
      const height = cropped
        ? Math.max(width * 0.625, visible * 1.35)
        : visible;
      setSize({ scale, height });
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [width, cropped]);

  return (
    <div ref={box} className={cn('overflow-hidden', className)}>
      <div
        className="flex origin-top-left flex-col"
        style={{
          width,
          height: size.height,
          transform: `scale(${size.scale})`,
          visibility: size.scale ? 'visible' : 'hidden',
        }}
      >
        {children}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------------ */
/* The frame: routes/_authenticated.tsx and -components/app-frame.tsx        */
/* ------------------------------------------------------------------------ */

const PROJECT = 'Checkout API';

function ProjectLogo() {
  return (
    <PlatformIcon
      platform="node"
      size={18}
      format="lg"
      className="-mx-px max-w-none shrink-0 rounded-md"
    />
  );
}

type Section = 'overview' | 'issues' | 'performance' | 'alerts';

const SECTION_LABEL: Record<Section, string> = {
  overview: 'Overview',
  issues: 'Issues',
  performance: 'Performance',
  alerts: 'Alerts',
};

/** The application around a screen: the project sidebar and the trail. */
export function AppFrame({
  section,
  trail = [],
  children,
  className,
}: {
  section: Section;
  /** Crumbs after the section's, for a page below it. */
  trail?: { label: string; mono?: boolean }[];
  children: ReactNode;
  className?: string;
}) {
  const item = (
    key: Section | 'other',
    icon: typeof IssuesIcon,
    label: string,
  ) => (
    <SidebarItem
      icon={icon}
      label={label}
      active={key === section}
      render={<span />}
    />
  );
  return (
    <AppShell
      className={cn('h-full', className)}
      collapseShortcut={false}
      sidebar={
        <Sidebar
          aria-label="Main navigation"
          header={
            <>
              <SidebarBrand
                collapseLabel="Collapse sidebar"
                expandLabel="Expand sidebar"
                render={<span />}
                switcher={
                  <SidebarProject
                    name={PROJECT}
                    detail="checkout-api"
                    tile={<ProjectLogo />}
                  />
                }
              />
              <SidebarSearch label="Search" shortcut="⌘K" />
            </>
          }
          footer={
            <>
              <SidebarItem
                icon={SettingsIcon}
                label="Settings"
                render={<span />}
              />
              <SidebarItem icon={DocsIcon} label="Docs" render={<span />} />
              <SidebarUser
                label="Account: ada@acme.dev"
                name="ada@acme.dev"
                detail="Admin"
                actions={[]}
              />
            </>
          }
        >
          {item('overview', OverviewIcon, 'Overview')}
          <SidebarGroup label="Triage">
            {item('issues', IssuesIcon, 'Issues')}
            {item('other', FeedbackIcon, 'User Feedback')}
            {item('other', ReleasesIcon, 'Releases')}
          </SidebarGroup>
          <SidebarGroup label="Monitor">
            {item('performance', PerformanceIcon, 'Performance')}
            {item('other', LogsIcon, 'Logs')}
            {item('other', CronsIcon, 'Crons')}
            {item('alerts', NotificationIcon, 'Alerts')}
            {item('other', AgentsIcon, 'Agents')}
          </SidebarGroup>
        </Sidebar>
      }
    >
      <Topbar>
        <Breadcrumbs
          items={[
            { label: PROJECT, icon: <ProjectLogo /> },
            { label: SECTION_LABEL[section] },
            ...trail,
          ]}
        />
      </Topbar>
      <div className="min-h-0 flex-1">{children}</div>
    </AppShell>
  );
}

/** The state a table owns before anyone types, sorts or pages. */
const INITIAL_QUERY: DataTableQuery = {
  sorting: [],
  filters: [],
  search: '',
  pagination: { pageIndex: 0, pageSize: 25 },
};

/* ------------------------------------------------------------------------ */
/* Issues: routes/.../issues/index.tsx, issues-table, issue-columns           */
/* ------------------------------------------------------------------------ */

interface Issue {
  id: string;
  title: string;
  culprit: string;
  level: 'error' | 'warning' | 'info';
  badge?: { label: string; tone: TagTone };
  trend: number[];
  events: string;
  users: string;
  priority: 'high' | 'medium' | 'low';
  assignee?: string;
  seen: string;
}

const t = (values: number[]) => values;

const ISSUES: Issue[] = [
  {
    id: 'CHECKOUT-API-4F',
    title: "TypeError: Cannot read properties of undefined (reading 'total')",
    culprit: 'src/checkout/summary.ts in applyDiscount',
    level: 'error',
    badge: { label: 'New', tone: 'brand' },
    trend: t([0, 0, 0, 0, 0, 0, 1, 2, 4, 7, 11, 16]),
    events: '12.8K',
    users: '1.2K',
    priority: 'high',
    assignee: 'ada@acme.dev',
    seen: '2m ago',
  },
  {
    id: 'CHECKOUT-API-3K',
    title: 'StripeCardError: Your card was declined.',
    culprit: 'src/payments/charge.ts in createCharge',
    level: 'error',
    trend: t([4, 6, 5, 7, 6, 8, 7, 9, 8, 7, 9, 8]),
    events: '2.2K',
    users: '1.9K',
    priority: 'medium',
    seen: '4m ago',
  },
  {
    id: 'CHECKOUT-API-21',
    title: 'PoolTimeoutError: Timed out acquiring a connection',
    culprit: 'src/db/pool.ts in acquire',
    level: 'error',
    badge: { label: 'New', tone: 'brand' },
    trend: t([1, 1, 2, 3, 5, 8, 9, 7, 6, 8, 10, 9]),
    events: '3.1K',
    users: '318',
    priority: 'high',
    assignee: 'lin@acme.dev',
    seen: '6m ago',
  },
  {
    id: 'CHECKOUT-API-2Z',
    title: 'AbortError: The operation was aborted',
    culprit: 'src/http/client.ts in fetchWithTimeout',
    level: 'warning',
    badge: { label: 'Regressed', tone: 'warning' },
    trend: t([2, 2, 3, 2, 4, 3, 5, 4, 6, 5, 7, 6]),
    events: '1.6K',
    users: '174',
    priority: 'medium',
    seen: '12m ago',
  },
  {
    id: 'CHECKOUT-API-1Q',
    title: 'ValidationError: Invalid currency code "EUR "',
    culprit: 'src/billing/invoice.ts in buildInvoice',
    level: 'warning',
    trend: t([3, 3, 2, 3, 3, 4, 3, 2, 3, 3, 4, 3]),
    events: '884',
    users: '96',
    priority: 'medium',
    seen: '31m ago',
  },
  {
    id: 'CHECKOUT-API-19',
    title: 'RangeError: Invalid time value',
    culprit: 'src/orders/format.ts in formatDelivery',
    level: 'error',
    badge: { label: 'New', tone: 'brand' },
    trend: t([0, 0, 0, 0, 1, 1, 2, 3, 2, 3, 4, 3]),
    events: '431',
    users: '77',
    priority: 'low',
    seen: '49m ago',
  },
  {
    id: 'CHECKOUT-API-15',
    title: 'Error: Webhook signature verification failed',
    culprit: 'src/webhooks/stripe.ts in verify',
    level: 'warning',
    trend: t([1, 2, 1, 1, 2, 1, 2, 1, 1, 2, 1, 1]),
    events: '262',
    users: '0',
    priority: 'low',
    seen: '2h ago',
  },
  {
    id: 'CHECKOUT-API-0X',
    title: "SyntaxError: Unexpected token '<' in JSON at position 0",
    culprit: 'src/integrations/tax.ts in fetchRates',
    level: 'error',
    badge: { label: 'New', tone: 'brand' },
    trend: t([0, 0, 1, 0, 1, 2, 1, 2, 1, 2, 2, 1]),
    events: '189',
    users: '41',
    priority: 'medium',
    seen: '3h ago',
  },
  {
    id: 'CHECKOUT-API-0R',
    title: 'Deprecation: Buffer() is deprecated, use Buffer.from()',
    culprit: 'node_modules/legacy-pdf/index.js',
    level: 'info',
    trend: t([1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]),
    events: '96',
    users: '—',
    priority: 'low',
    seen: '5h ago',
  },
];

const LEVEL_MARK = {
  error: 'bg-sev-error',
  warning: 'bg-sev-warning',
  info: 'bg-sev-info',
} as const;

const PRIORITY_TONE: Record<Issue['priority'], TagTone> = {
  high: 'error',
  medium: 'warning',
  low: 'neutral',
};

const PRIORITY_LABEL: Record<Issue['priority'], string> = {
  high: 'High',
  medium: 'Medium',
  low: 'Low',
};

const issueColumn = createDataTableColumnHelper<Issue>();

const ISSUE_COLUMNS = issueColumn.columns([
  issueColumn.accessor('title', {
    id: 'issue',
    header: 'Issue',
    enableHiding: false,
    enableSorting: false,
    cell: ({ row }) => {
      const issue = row.original;
      return (
        <span className="flex min-w-0 items-center gap-3 py-2">
          <span
            aria-hidden="true"
            className={cn(
              'h-8 w-0.75 shrink-0 rounded-pill',
              LEVEL_MARK[issue.level],
            )}
          />
          <span className="flex min-w-0 flex-col gap-0.5">
            <span className="flex min-w-0 items-center gap-2">
              <span className="min-w-0 truncate font-medium text-fg text-value">
                {issue.title}
              </span>
              {issue.badge ? (
                <Tag
                  variant="soft"
                  tone={issue.badge.tone}
                  className="shrink-0"
                >
                  {issue.badge.label}
                </Tag>
              ) : null}
            </span>
            <Text variant="mono-sm" tone="ghost" truncate>
              {issue.id} · {issue.culprit}
            </Text>
          </span>
        </span>
      );
    },
    meta: { label: 'Issue' },
  }),
  issueColumn.accessor('trend', {
    id: 'trend',
    header: 'Trend',
    enableSorting: false,
    cell: ({ getValue }) => (
      <Sparkline values={getValue()} label="Trend" className="h-6 w-full" />
    ),
    meta: { label: 'Trend', width: 120 },
  }),
  issueColumn.accessor('events', {
    id: 'events',
    header: 'Events',
    cell: ({ getValue }) => getValue(),
    meta: { label: 'Events', width: 92, align: 'end', numeric: true },
  }),
  issueColumn.accessor('users', {
    id: 'users',
    header: 'Users',
    enableSorting: false,
    cell: ({ getValue }) => getValue(),
    meta: { label: 'Users', width: 84, align: 'end', numeric: true },
  }),
  issueColumn.accessor('priority', {
    id: 'priority',
    header: 'Priority',
    cell: ({ getValue }) => (
      <Tag variant="soft" tone={PRIORITY_TONE[getValue()]}>
        {PRIORITY_LABEL[getValue()]}
      </Tag>
    ),
    meta: { label: 'Priority', width: 104 },
  }),
  issueColumn.accessor('assignee', {
    id: 'assignee',
    header: 'Assignee',
    enableSorting: false,
    cell: ({ getValue }) => {
      const email = getValue();
      return email ? (
        <span className="flex min-w-0 items-center gap-2">
          <Avatar name={email} size="xs" />
          <Text variant="meta" truncate>
            {email.split('@')[0]}
          </Text>
        </span>
      ) : (
        <span className="text-fg-ghost">–</span>
      );
    },
    meta: { label: 'Assignee', width: 148 },
  }),
  issueColumn.accessor('seen', {
    id: 'lastSeen',
    header: 'Last seen',
    cell: ({ getValue }) => getValue(),
    meta: { label: 'Last seen', width: 120, align: 'end' },
  }),
]);

const ISSUE_ROW_MENU = () => [];

export function IssuesScreen({ issues = ISSUES }: { issues?: Issue[] }) {
  const [query, setQuery] = useState(INITIAL_QUERY);
  const table = useDataTable({
    data: issues,
    columns: ISSUE_COLUMNS,
    rowCount: 84,
    query,
    onQueryChange: (update) => setQuery(update),
    getRowId: (issue) => issue.id,
    enableSelection: true,
    rowMenu: ISSUE_ROW_MENU,
  });

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Issues"
        summary={[
          `Errors from ${PROJECT}, grouped by what caused them`,
          '84 issues',
        ]}
        tabs={
          <Tabs value="open">
            <TabList>
              <Tab value="open">Open</Tab>
              <Tab value="resolved">Resolved</Tab>
              <Tab value="muted">Muted</Tab>
              <Tab value="all">All</Tab>
            </TabList>
          </Tabs>
        }
      />
      <Page scroll={false}>
        <div className="flex min-h-0 flex-1 flex-col gap-3">
          <div className="flex items-center gap-2">
            <QueryBar
              className="min-w-0 flex-1"
              fields={[]}
              filters={query.filters}
              search={query.search}
              onChange={() => {}}
            />
            <DataTableColumnsButton table={table} />
          </div>
          <DataTable table={table} className="min-h-0 flex-1" />
        </div>
      </Page>
    </div>
  );
}

/* ------------------------------------------------------------------------ */
/* An issue: issues/$issueId/events/$eventId.tsx and its -components         */
/* ------------------------------------------------------------------------ */

const FRAMES = [
  {
    fn: 'applyDiscount',
    file: 'src/checkout/summary.ts:42:18',
    app: true,
    lines: [
      [40, 'export function applyDiscount(cart: Cart, code: string) {'],
      [41, '  const promo = promotions.get(code);'],
      [42, '  const total = promo.rules.total - cart.discount;'],
      [43, '  return { ...cart, total };'],
      [44, '}'],
    ] as const,
    hit: 42,
  },
  { fn: 'buildSummary', file: 'src/checkout/summary.ts:18:9', app: true },
  { fn: 'handle', file: 'src/routes/checkout.ts:57:22', app: true },
  {
    fn: 'Layer.handle [as handle_request]',
    file: 'node_modules/express/lib/router/layer.js:95:5',
    app: false,
  },
  {
    fn: 'next',
    file: 'node_modules/express/lib/router/route.js:149:13',
    app: false,
  },
];

/** `EventHighlights`: the facts worth reading first, in two columns. */
const HIGHLIGHTS: [string, string, boolean?][] = [
  ['handled', 'no'],
  ['level', 'error'],
  ['transaction', 'POST /v1/checkout', true],
  ['url', 'https://api.acme.dev/v1/checkout', true],
  ['environment', 'production'],
  ['release', 'checkout-api@2.4.0', true],
  ['runtime', 'node v22.11.0'],
  ['trace id', '4bf92f3577b34da6a3ce929d0e0e4736', true],
];

/** `BreadcrumbTimeline`: what happened before the error, oldest first. */
const CRUMBS: {
  icon: 'http' | 'console' | 'click' | 'error';
  category: string;
  message: string;
  time: string;
}[] = [
  {
    icon: 'click',
    category: 'ui.click',
    message: 'button#apply-promo',
    time: '14:02:11',
  },
  {
    icon: 'http',
    category: 'http',
    message: 'GET /v1/promotions/SPRING25 → 404',
    time: '14:02:11',
  },
  {
    icon: 'console',
    category: 'console',
    message: 'Promotion SPRING25 not found, continuing',
    time: '14:02:12',
  },
  {
    icon: 'http',
    category: 'http',
    message: 'POST /v1/checkout → 500',
    time: '14:02:12',
  },
  {
    icon: 'error',
    category: 'exception',
    message: "TypeError: Cannot read properties of undefined (reading 'total')",
    time: '14:02:12',
  },
];

/** `TagDistribution`: each tag, its leading value and the split. */
const TAG_SPLIT: { key: string; top: string; shares: number[] }[] = [
  { key: 'environment', top: 'production', shares: [0.94, 0.06] },
  { key: 'release', top: 'checkout-api@2.4.0', shares: [0.81, 0.15, 0.04] },
  { key: 'server_name', top: 'checkout-api-7d9f4', shares: [0.38, 0.33, 0.29] },
  { key: 'runtime', top: 'node v22.11.0', shares: [1] },
];

const SEGMENTS = ['bg-chart-1', 'bg-chart-2', 'bg-chart-3'] as const;

/** The issue's events per day over the last thirty, for `EventTrends`. */
const DAILY = series(30, 13, 120, 0.9).map((events, index) => ({
  t: Date.UTC(2026, 9, 4 + index),
  events: index < 22 ? Math.round(events * 0.15) : events,
}));

/** `Section` from shared/ui/components/collapsible-section.tsx, open. */
function EventSection({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="border-border-divider border-b last:border-b-0">
      <div className="flex h-12 items-center gap-2 px-5">
        <ChevronRightIcon
          size="sm"
          className="shrink-0 rotate-90 text-fg-ghost"
        />
        <h2 className="min-w-0 flex-1 truncate text-card-title text-fg">
          {title}
        </h2>
      </div>
      <div className="px-5 pb-5">{children}</div>
    </section>
  );
}

/** `Property` from features/issue/ui/components/issue-properties.tsx. */
function Property({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex min-h-9 items-center justify-between gap-4">
      <dt className="shrink-0 text-fg-meta text-meta">{label}</dt>
      <dd className="flex min-w-0 justify-end text-fg text-value">
        {children}
      </dd>
    </div>
  );
}

function RailBlock({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="border-border-divider border-b px-5 py-4 last:border-b-0">
      <h2 className="mb-2 text-column text-fg-meta">{title}</h2>
      {children}
    </section>
  );
}

/**
 * An issue, opened on its latest event: issues/$issueId/events/$eventId.tsx.
 * The header and action bar, then the event column (thirty days of events,
 * the event navigator, and the event's sections) beside the issue's rail.
 * `rail={false}` drops the rail, for a window too narrow for it.
 */
// A static screenshot of one page, read top to bottom: splitting it would
// only scatter the layout across helpers nothing else uses.
// react-doctor-disable-next-line react-doctor/no-giant-component
export function IssueScreen({
  rail = 'open',
  at = 'top',
}: {
  /** The details rail open, folded to its strip, or not drawn at all. */
  rail?: 'open' | 'folded' | 'none';
  /** Scrolled to the stack trace, as after clicking its tab in the bar. */
  at?: 'top' | 'stack';
}) {
  const scrolled = at === 'stack';
  return (
    <div className="flex h-full flex-col">
      <header className="shrink-0 border-border-divider border-b px-page-gutter pt-6 pb-5">
        <div className="flex items-start justify-between gap-8">
          <div className="flex min-w-0 flex-1 gap-4">
            <span className="w-1 shrink-0 self-stretch rounded-pill bg-sev-error" />
            <div className="flex min-w-0 flex-col gap-1">
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-mono text-fg-meta text-mono-sm">
                  CHECKOUT-API-4F
                </span>
                <Tag variant="soft" tone="warning">
                  Unresolved
                </Tag>
                <Text variant="meta" tone="subtle">
                  error
                </Text>
              </div>
              <h1 className="truncate text-fg text-page-title">TypeError</h1>
              <p className="truncate font-mono text-fg-secondary text-mono">
                Cannot read properties of undefined (reading &apos;total&apos;)
              </p>
              <p className="mt-1 flex flex-wrap items-center gap-x-4 gap-y-0.5 text-fg-subtle text-meta">
                <span className="truncate font-mono">
                  src/checkout/summary.ts in applyDiscount
                </span>
                <span>First seen 3 hours ago</span>
                <span>Last seen 2 minutes ago</span>
              </p>
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            {/* IssueActionBar in features/issue/ui/components/issue-actions.tsx. */}
            <SplitButton
              icon={ResolveIcon}
              menuLabel="More resolve options"
              actions={[
                { id: 'resolve', label: 'Resolve' },
                { id: 'next', label: 'Resolve in next release' },
              ]}
            >
              Resolve
            </SplitButton>
            <Button variant="secondary" icon={MuteIcon}>
              Archive
            </Button>
            <Button variant="ghost" icon={StarIcon} aria-label="Bookmark" />
            <Button
              variant="ghost"
              icon={NotificationIcon}
              aria-label="Subscribe"
            />
            <Button variant="ghost" icon={MoreIcon} aria-label="More" />
          </div>
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        <main className="min-w-0 flex-1 overflow-hidden">
          <div className="flex flex-col gap-5 px-page-gutter pt-5 pb-8">
            {/* EventTrends */}
            <section
              hidden={scrolled}
              className="rounded-xl border border-border-subtle bg-surface"
            >
              <header className="flex items-end justify-between gap-4 px-5 pt-4">
                <div>
                  <h2 className="text-card-title text-fg">
                    Events, last 30 days
                  </h2>
                  <Text variant="meta" tone="subtle">
                    Every event this issue grouped, per day
                  </Text>
                </div>
                <dl className="flex flex-col items-end">
                  <dt className="text-fg-meta text-hint">Events</dt>
                  <dd className="text-fg text-section tabular-nums">
                    {NUMBER.format(DAILY.reduce((sum, d) => sum + d.events, 0))}
                  </dd>
                </dl>
              </header>
              <div className="px-3 pt-3 pb-4">
                <BarsChart
                  data={DAILY}
                  series={[
                    { key: 'events', label: 'Events', color: 'var(--chart-1)' },
                  ]}
                  xKey="t"
                  height={140}
                  formatX={dayLabel}
                  formatY={(v) => COMPACT.format(v)}
                  label="Events, last 30 days"
                />
              </div>
            </section>

            {/* EventNavigator */}
            <div
              className={cn(
                '-mx-page-gutter border-border-divider border-b bg-panel/95 px-page-gutter pt-3',
                // Stuck to the top of the column once scrolled, as it is.
                scrolled && '-mt-5',
              )}
            >
              <div className="flex items-center justify-between gap-x-6 pb-1">
                <div className="flex items-center gap-3">
                  <div className="inline-flex items-center">
                    <Button
                      variant="secondary"
                      size="sm"
                      icon={ChevronLeftIcon}
                      aria-label="Previous event"
                      className="rounded-e-none"
                    />
                    <Button
                      variant="secondary"
                      size="sm"
                      icon={ChevronRightIcon}
                      aria-label="Next event"
                      disabled
                      className="-ms-px rounded-s-none"
                    />
                  </div>
                  <Text
                    variant="value"
                    className="whitespace-nowrap text-fg tabular-nums"
                  >
                    Event 214 of 214
                  </Text>
                  <div className="inline-flex items-center">
                    <Button
                      variant="secondary"
                      size="sm"
                      className="rounded-e-none"
                    >
                      First
                    </Button>
                    <Button
                      variant="secondary"
                      size="sm"
                      disabled
                      className="-ms-px rounded-s-none"
                    >
                      Latest
                    </Button>
                  </div>
                </div>
                <div className="flex min-w-0 items-center gap-2 overflow-hidden whitespace-nowrap">
                  <span className="font-mono text-fg-subtle text-mono-sm">
                    9f3c2a71
                  </span>
                  <Text variant="meta" tone="subtle">
                    2 minutes ago
                  </Text>
                  <Tag variant="soft" verbatim>
                    production
                  </Tag>
                  <Tag variant="soft" verbatim className="font-mono">
                    checkout-api@2.4.0
                  </Tag>
                  <Tag variant="soft" verbatim>
                    node
                  </Tag>
                </div>
              </div>
              <nav className="-mb-px mt-1 flex gap-5">
                {[
                  'Highlights',
                  'Stack Trace',
                  'Breadcrumbs',
                  'Tags',
                  'Context',
                  'Event Details',
                ].map((jump, index) => (
                  <span
                    key={jump}
                    className={cn(
                      'relative flex h-9 shrink-0 items-center whitespace-nowrap text-meta',
                      "after:absolute after:inset-x-0 after:bottom-0 after:h-0.5 after:rounded-pill after:content-['']",
                      index === 1
                        ? 'font-medium text-fg after:bg-surface-brand'
                        : 'text-fg-subtle',
                    )}
                  >
                    {jump}
                  </span>
                ))}
              </nav>
            </div>

            {/* EventSections */}
            <div className="overflow-hidden rounded-xl border border-border-subtle bg-surface">
              {scrolled ? null : (
                <EventSection title="Highlights">
                  <div className="grid grid-cols-2 gap-x-8">
                    {[HIGHLIGHTS.slice(0, 4), HIGHLIGHTS.slice(4)].map(
                      (column) => (
                        <dl key={column[0][0]} className="text-sm">
                          {column.map(([label, value, mono]) => (
                            <div
                              key={label}
                              className="flex items-center gap-4 rounded px-2 py-1.5 odd:bg-surface-hover/40"
                            >
                              <dt className="w-28 shrink-0 text-fg-subtle">
                                {label}
                              </dt>
                              <dd
                                className={cn(
                                  'min-w-0 truncate text-fg',
                                  mono && 'font-mono text-xs',
                                )}
                              >
                                {value}
                              </dd>
                            </div>
                          ))}
                        </dl>
                      ),
                    )}
                  </div>
                </EventSection>
              )}
              <EventSection title="Stack Trace">
                <div className="flex flex-col gap-4">
                  <div className="flex flex-col gap-1">
                    <h3 className="font-mono text-section text-sev-error-fg">
                      TypeError
                    </h3>
                    <p className="font-mono text-fg-secondary text-mono">
                      Cannot read properties of undefined (reading
                      &apos;total&apos;)
                    </p>
                  </div>
                  <div className="divide-y divide-border-divider overflow-hidden rounded-lg border border-border-subtle">
                    {FRAMES.map((frame, index) => (
                      <div
                        key={frame.fn}
                        className={frame.app ? 'bg-surface' : 'bg-panel'}
                      >
                        <div className="flex items-center gap-4 px-4 py-2.5">
                          <span className="w-6 shrink-0 font-mono text-fg-ghost text-mono-sm tabular-nums">
                            {String(index + 1).padStart(2, '0')}
                          </span>
                          <div className="min-w-0 flex-1">
                            <p
                              className={cn(
                                'truncate font-mono text-mono',
                                frame.app
                                  ? 'font-medium text-fg'
                                  : 'text-fg-subtle',
                              )}
                            >
                              {frame.fn}
                            </p>
                            <p className="truncate font-mono text-fg-meta text-mono-sm">
                              {frame.file}
                            </p>
                          </div>
                          {frame.app ? (
                            <span className="text-fg-ghost">
                              {'lines' in frame ? (
                                <ChevronDownIcon className="size-4" />
                              ) : (
                                <ChevronRightIcon className="size-4" />
                              )}
                            </span>
                          ) : null}
                        </div>
                        {'lines' in frame && frame.lines ? (
                          <div className="border-border-divider border-t bg-code py-1.5 font-mono text-mono-sm leading-relaxed">
                            {frame.lines.map(([n, code]) => (
                              <div
                                key={n}
                                className={cn(
                                  'relative flex',
                                  n === frame.hit
                                    ? 'bg-sev-error-surface'
                                    : 'opacity-70',
                                )}
                              >
                                {n === frame.hit ? (
                                  <div className="absolute inset-y-0 left-0 w-0.5 bg-sev-error" />
                                ) : null}
                                <span
                                  className={cn(
                                    'w-12 shrink-0 py-0.5 pr-4 pl-3 text-right',
                                    n === frame.hit
                                      ? 'font-medium text-sev-error-fg'
                                      : 'text-fg-ghost',
                                  )}
                                >
                                  {n}
                                </span>
                                <span className="whitespace-pre py-0.5 pr-4 text-code-fg">
                                  {highlight(code)}
                                </span>
                              </div>
                            ))}
                          </div>
                        ) : null}
                      </div>
                    ))}
                  </div>
                </div>
              </EventSection>
              <EventSection title="Breadcrumbs">
                <ol className="divide-y divide-border-divider overflow-hidden rounded-lg border border-border-subtle">
                  {CRUMBS.map((crumb) => {
                    const Icon = {
                      http: NetworkIcon,
                      console: ConsoleIcon,
                      click: ClickIcon,
                      error: ErrorIcon,
                    }[crumb.icon];
                    return (
                      <li
                        key={crumb.message}
                        className="flex items-center gap-3 bg-surface px-3 py-2"
                      >
                        <Icon
                          className={cn(
                            'size-3.5 shrink-0',
                            crumb.icon === 'error'
                              ? 'text-sev-error-fg'
                              : 'text-fg-subtle',
                          )}
                        />
                        <span className="w-28 shrink-0 truncate font-mono text-fg-meta text-mono-sm">
                          {crumb.category}
                        </span>
                        <span className="min-w-0 flex-1 truncate text-fg-secondary text-value">
                          {crumb.message}
                        </span>
                        <time className="shrink-0 font-mono text-fg-ghost text-mono-sm tabular-nums">
                          {crumb.time}
                        </time>
                      </li>
                    );
                  })}
                </ol>
              </EventSection>
            </div>
          </div>
        </main>

        {/* IssueRail, folded: the strip with the way back in. */}
        {rail === 'folded' ? (
          <div className="flex w-12 shrink-0 justify-center border-border-divider border-l bg-aside pt-3">
            <Button
              variant="ghost"
              size="sm"
              icon={CollapsePanelIcon}
              aria-label="Open sidebar"
            />
          </div>
        ) : null}

        {/* IssueRail and EventRail */}
        <aside
          hidden={rail !== 'open'}
          className="w-88 shrink-0 overflow-hidden border-border-divider border-l bg-aside"
        >
          <div className="flex h-11 items-center justify-between border-border-divider border-b ps-5 pe-3">
            <span className="text-card-title text-fg">Details</span>
            <Button
              variant="ghost"
              size="sm"
              icon={ExpandPanelIcon}
              aria-label="Hide sidebar"
            />
          </div>
          <RailBlock title="Properties">
            <dl className="flex flex-col">
              <Property label="Status">
                <span className="inline-flex items-center gap-2">
                  <span className="size-2 shrink-0 rounded-pill bg-sev-warning" />
                  Unresolved
                </span>
              </Property>
              <Property label="Priority">
                <span className="inline-flex items-center gap-2">
                  <span className="size-2 shrink-0 rounded-pill bg-sev-error" />
                  High
                </span>
              </Property>
              <Property label="Assignee">
                <span className="flex items-center gap-2">
                  <Avatar name="ada@acme.dev" size="xs" />
                  Me
                </span>
              </Property>
              <div className="my-2 h-px bg-border-divider" />
              <Property label="Events">
                <span className="tabular-nums">12,812</span>
              </Property>
              <Property label="Users">
                <span className="tabular-nums">1,204</span>
              </Property>
              <div className="my-2 h-px bg-border-divider" />
              <Property label="First seen">3 hours ago</Property>
              <Property label="Last seen">2 minutes ago</Property>
            </dl>
          </RailBlock>
          <RailBlock title="Tags">
            <ul className="flex flex-col gap-3">
              {TAG_SPLIT.map((tag) => (
                <li key={tag.key} className="flex flex-col gap-1.5">
                  <span className="flex items-baseline justify-between gap-3">
                    <Text variant="meta" tone="meta" truncate>
                      {tag.key}
                    </Text>
                    <span className="flex min-w-0 items-baseline gap-2">
                      <Text
                        variant="mono-sm"
                        truncate
                        className="max-w-40 text-fg"
                      >
                        {tag.top}
                      </Text>
                      <Text
                        variant="meta"
                        tone="subtle"
                        className="tabular-nums"
                      >
                        {Math.round(tag.shares[0] * 100)}%
                      </Text>
                    </span>
                  </span>
                  <span className="flex h-1.5 overflow-hidden rounded-pill bg-surface-chip">
                    {tag.shares.map((share, index) => (
                      <span
                        // Fixed segments of a fixed bar.
                        // react-doctor-disable-next-line react-doctor/no-array-index-as-key
                        key={index}
                        className={SEGMENTS[index]}
                        style={{ width: `${share * 100}%` }}
                      />
                    ))}
                  </span>
                </li>
              ))}
            </ul>
          </RailBlock>
        </aside>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------------ */
/* Performance: routes/.../performance/index.tsx and endpoints-table.tsx     */
/* ------------------------------------------------------------------------ */

interface Endpoint {
  name: string;
  op: string;
  share: number;
  calls: number;
  avg: number;
  p95: number;
  failures: number;
}

const ENDPOINTS: Endpoint[] = [
  {
    name: 'GET /v1/carts/:id',
    op: 'http.server',
    share: 0.33,
    calls: 312_900,
    avg: 119,
    p95: 860,
    failures: 0.004,
  },
  {
    name: 'GET /v1/products',
    op: 'http.server',
    share: 0.21,
    calls: 884_200,
    avg: 72,
    p95: 284,
    failures: 0,
  },
  {
    name: 'GET /v1/sessions',
    op: 'http.server',
    share: 0.14,
    calls: 204_600,
    avg: 140,
    p95: 544,
    failures: 0,
  },
  {
    name: 'POST /v1/payments',
    op: 'http.server',
    share: 0.11,
    calls: 48_200,
    avg: 466,
    p95: 1840,
    failures: 0.064,
  },
  {
    name: 'POST /v1/checkout',
    op: 'http.server',
    share: 0.07,
    calls: 51_100,
    avg: 263,
    p95: 716,
    failures: 0.012,
  },
  {
    name: 'checkout.consolidate',
    op: 'function',
    share: 0.019,
    calls: 96_600,
    avg: 40,
    p95: 142,
    failures: 0,
  },
  {
    name: 'jobs.reconcileLedger',
    op: 'queue.task',
    share: 0.011,
    calls: 732,
    avg: 3060,
    p95: 9340,
    failures: 0.117,
  },
  {
    name: 'POST /v1/webhooks/stripe',
    op: 'http.server',
    share: 0.009,
    calls: 19_800,
    avg: 104,
    p95: 422,
    failures: 0.021,
  },
];

const percent = (value: number, digits: number) =>
  new Intl.NumberFormat('en', {
    style: 'percent',
    maximumFractionDigits: digits,
  }).format(value);

const compact = (value: number) => COMPACT.format(value);

/** `failureTone` in features/transaction/model/endpoints.ts. */
function failureClass(rate: number) {
  if (rate >= 0.05) return 'text-sev-error-fg';
  if (rate >= 0.01) return 'text-sev-warning-fg';
  return 'text-fg-ghost tabular-nums';
}

const endpointColumn = createDataTableColumnHelper<Endpoint>();
const duration = (ms: number) => (
  <span className="text-fg-secondary tabular-nums">
    {formatSpanDuration(ms)}
  </span>
);

const ENDPOINT_COLUMNS = endpointColumn.columns([
  endpointColumn.accessor('name', {
    id: 'endpoint',
    header: 'Endpoint',
    enableHiding: false,
    enableSorting: false,
    cell: ({ row }) => (
      <span className="flex min-w-0 items-baseline gap-2">
        <span className="truncate font-mono text-fg text-mono">
          {row.original.name}
        </span>
        <span className="shrink-0 text-fg-subtle text-meta">
          {row.original.op}
        </span>
      </span>
    ),
    meta: { label: 'Endpoint' },
  }),
  endpointColumn.accessor('share', {
    id: 'time_spent',
    header: 'Time spent',
    cell: ({ getValue }) => (
      <span className="flex w-full items-center gap-2">
        <span className="h-1.5 min-w-0 flex-1 overflow-hidden rounded-pill bg-surface-chip">
          <span
            className="block h-full rounded-pill bg-chart-1"
            style={{ width: `${Math.max(getValue() * 100, 1.5)}%` }}
          />
        </span>
        <span className="w-11 shrink-0 text-right text-fg-secondary text-meta tabular-nums">
          {percent(getValue(), getValue() < 0.1 ? 1 : 0)}
        </span>
      </span>
    ),
    meta: { label: 'Time spent', width: 168 },
  }),
  endpointColumn.accessor('calls', {
    id: 'count',
    header: 'Calls',
    cell: ({ getValue }) => (
      <span className="text-fg-secondary tabular-nums">
        {compact(getValue())}
      </span>
    ),
    meta: { label: 'Calls', width: 96, align: 'end' },
  }),
  endpointColumn.accessor('avg', {
    id: 'avg',
    header: 'Average',
    cell: ({ getValue }) => duration(getValue()),
    meta: { label: 'Average', width: 92, align: 'end' },
  }),
  endpointColumn.accessor('p95', {
    id: 'p95',
    header: 'p95',
    enableSorting: false,
    cell: ({ getValue }) => duration(getValue()),
    meta: { label: 'p95', width: 88, align: 'end' },
  }),
  endpointColumn.accessor('failures', {
    id: 'failure_rate',
    header: 'Failures',
    cell: ({ getValue }) => (
      <span className={failureClass(getValue())}>{percent(getValue(), 1)}</span>
    ),
    meta: { label: 'Failures', width: 96, align: 'end' },
  }),
]);

export function PerformanceScreen() {
  const [query, setQuery] = useState(INITIAL_QUERY);
  const table = useDataTable({
    data: ENDPOINTS,
    columns: ENDPOINT_COLUMNS,
    rowCount: ENDPOINTS.length,
    query,
    onQueryChange: (update) => setQuery(update),
    getRowId: (row) => `${row.op} ${row.name}`,
  });

  return (
    <div className="flex h-full flex-col">
      <PageHeader title="Performance" summary={[PROJECT, '8 endpoints']} />
      <Page scroll={false}>
        <div className="flex min-h-0 flex-1 flex-col gap-3">
          <div className="flex items-center gap-2">
            <QueryBar
              className="min-w-0"
              fields={[]}
              filters={query.filters}
              search={query.search}
              placeholder="Search endpoints…"
              onChange={() => {}}
            />
            <div className="flex flex-1 items-center gap-2 sm:flex-none">
              {/* `ScopeSegment` from shared/ui/components/scope-bar.tsx. */}
              <div className="flex rounded-lg border border-border-subtle bg-surface">
                <span className="flex h-control-md items-center gap-1.5 px-2.5 text-control text-fg-subtle">
                  <TimeIcon size="sm" />
                  14d
                  <ChevronDownIcon
                    size="sm"
                    className="ms-auto text-fg-ghost"
                  />
                </span>
              </div>
              <DataTableColumnsButton table={table} />
            </div>
          </div>
          <DataTable table={table} className="min-h-0 flex-1" />
        </div>
      </Page>
    </div>
  );
}

/* ------------------------------------------------------------------------ */
/* Alerts: routes/.../alerts/index.tsx and rule-list.tsx                     */
/* ------------------------------------------------------------------------ */

const RULES = [
  {
    name: 'Regression after a deploy',
    type: { icon: UnresolveIcon, label: 'Regression' },
    conditions: 'production · error or higher',
    channels: [
      { icon: HashIcon, label: '#checkout-alerts' },
      { icon: WebhookIcon, label: 'Incident webhook' },
    ],
    frequency: 'Every 30 minutes',
    last: '12 minutes ago',
    enabled: true,
  },
  {
    name: 'New issue in production',
    type: { icon: IssuesIcon, label: 'New issue' },
    conditions: 'production · error or higher · after 10 events',
    channels: [
      { icon: HashIcon, label: '#checkout-alerts' },
      { icon: MailIcon, label: 'On-call email' },
    ],
    frequency: 'Every 30 minutes',
    last: '2 hours ago',
    enabled: true,
  },
  {
    name: 'Muted issue comes back',
    type: { icon: NotificationIcon, label: 'Reactivated' },
    conditions: 'All',
    channels: [{ icon: WebhookIcon, label: 'Incident webhook' }],
    frequency: 'Once an hour',
    last: 'Yesterday',
    enabled: true,
  },
  {
    name: 'Feedback from paying users',
    type: { icon: FeedbackIcon, label: 'New feedback' },
    conditions: 'production',
    channels: [{ icon: MailIcon, label: 'Support inbox' }],
    frequency: 'Once a day',
    last: '3 days ago',
    enabled: false,
  },
];

const RULE_COLUMNS =
  'grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)_minmax(0,1fr)_minmax(0,1.2fr)_8rem_8rem_3rem]';

export function AlertsScreen() {
  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Alerts"
        summary={[PROJECT, '3 alerts active']}
        actions={
          <div className="flex items-center gap-2">
            <Button icon={IntegrationsIcon}>4 channels</Button>
            <Button variant="primary" icon={NewIcon}>
              New alert
            </Button>
          </div>
        }
        tabs={
          <Tabs value="rules">
            <TabList>
              <Tab value="rules">Alerts</Tab>
              <Tab value="deliveries">Deliveries</Tab>
            </TabList>
          </Tabs>
        }
      />
      <Page scroll={false}>
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border bg-panel">
          <div
            className={cn(
              'grid shrink-0 gap-4 border-border-divider border-b px-4 py-2.5 text-column text-fg-meta',
              RULE_COLUMNS,
            )}
          >
            <span>Alert</span>
            <span>When</span>
            <span>Only if</span>
            <span>Notifies</span>
            <span>At most</span>
            <span className="text-right">Last triggered</span>
            <span className="sr-only">Active</span>
          </div>
          <ul className="divide-y divide-border-divider">
            {RULES.map((rule) => (
              <li key={rule.name}>
                <div
                  className={cn(
                    'grid items-center gap-x-4 px-4 py-3',
                    RULE_COLUMNS,
                    !rule.enabled && 'opacity-60',
                  )}
                >
                  <span className="flex min-w-0 items-center gap-3">
                    <span
                      className={cn(
                        'h-8 w-0.75 shrink-0 rounded-pill',
                        rule.enabled ? 'bg-success' : 'bg-border-control',
                      )}
                    />
                    <span className="truncate font-medium text-body text-fg">
                      {rule.name}
                    </span>
                  </span>
                  <span className="flex min-w-0 items-center gap-2 text-fg-secondary text-meta">
                    <rule.type.icon className="size-3.5 shrink-0 text-fg-subtle" />
                    <span className="truncate">{rule.type.label}</span>
                  </span>
                  <span className="truncate text-fg-secondary text-meta">
                    {rule.conditions}
                  </span>
                  <span className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-fg-secondary text-meta">
                    {rule.channels.map((channel) => (
                      <span
                        key={channel.label}
                        className="flex min-w-0 items-center gap-1.5"
                      >
                        <channel.icon className="size-3.5 shrink-0 text-fg-subtle" />
                        <span className="truncate">{channel.label}</span>
                      </span>
                    ))}
                  </span>
                  <span className="text-fg-secondary text-meta">
                    {rule.frequency}
                  </span>
                  <span className="text-right text-fg-subtle text-meta tabular-nums">
                    {rule.last}
                  </span>
                  <span className="flex justify-end">
                    <Switch checked={rule.enabled} aria-label={rule.name} />
                  </span>
                </div>
              </li>
            ))}
          </ul>
        </div>
      </Page>
    </div>
  );
}

/* ------------------------------------------------------------------------ */
/* Overview: routes/.../projects/$id/index.tsx and -components/overview-*     */
/* ------------------------------------------------------------------------ */

/** A fixed pseudo-random walk, so server and client draw the same bars. */
function series(length: number, seed: number, base: number, swing: number) {
  let value = seed;
  return Array.from({ length }, (_, index) => {
    value = (value * 9301 + 49297) % 233280;
    const noise = value / 233280;
    const wave = Math.sin(index / 5) * 0.35 + 1;
    const spike = index > length - 12 ? 1 + (index - (length - 12)) * 0.12 : 1;
    return Math.round(base * wave * spike * (0.6 + noise * swing));
  });
}

const BUCKETS = 56;
const START = Date.UTC(2026, 9, 19);
const ERRORS = series(BUCKETS, 11, 4200, 0.9);
const WARNINGS = series(BUCKETS, 23, 1600, 0.8);
const INFOS = series(BUCKETS, 37, 500, 0.6);
const VOLUME = ERRORS.map((error, index) => ({
  t: START + index * 6 * 3600_000,
  error,
  warning: WARNINGS[index],
  info: INFOS[index],
}));

const dayLabel = (value: string | number) => DAY.format(new Date(value));

/** `MetricDeltaText` with `polarity="up-is-bad"`. */
function Delta({ change }: { change: number }) {
  const Arrow = change > 0 ? TrendUpIcon : TrendDownIcon;
  return (
    <span
      className={cn(
        'inline-flex items-center gap-0.5 font-medium text-xs tabular-nums',
        change > 0 ? 'text-sev-error-fg' : 'text-success-fg',
      )}
    >
      <Arrow className="size-3" aria-hidden />
      {change > 0 ? '+' : ''}
      {change.toFixed(1)}%
    </span>
  );
}

function Kpi({
  label,
  value,
  delta,
  footnote,
  trend,
}: {
  label: string;
  value: string;
  delta?: ReactNode;
  footnote: string;
  trend?: number[];
}) {
  return (
    <div className="flex min-w-0 flex-col gap-2 rounded-2xl border border-border-subtle bg-surface px-5 pt-4 pb-4.5">
      <div className="flex items-center justify-between gap-2">
        <span className="truncate text-fg-meta text-meta">{label}</span>
        {delta}
      </div>
      <div className="flex items-end justify-between gap-4">
        <span className="text-fg text-page-title tabular-nums">{value}</span>
        {trend ? (
          <Sparkline
            values={trend}
            tone="danger"
            label={label}
            className="h-8 w-28 shrink-0"
          />
        ) : null}
      </div>
      <p className="truncate text-fg-subtle text-hint">{footnote}</p>
    </div>
  );
}

function Panel({
  title,
  description,
  actions,
  children,
}: {
  title: string;
  description?: string;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="flex h-full min-w-0 flex-col rounded-2xl border border-border-subtle bg-surface">
      <header className="flex items-start justify-between gap-3 px-5 pt-4">
        <div className="min-w-0">
          <h2 className="text-card-title text-fg">{title}</h2>
          {description ? (
            <p className="mt-0.5 text-fg-subtle text-meta">{description}</p>
          ) : null}
        </div>
        {actions}
      </header>
      <div className="min-w-0 flex-1 px-5 pt-4 pb-5">{children}</div>
    </section>
  );
}

/** `ScopeSegment` from shared/ui/components/scope-bar.tsx, at rest. */
function Scope({
  icon: Icon,
  label,
}: {
  icon: typeof TimeIcon;
  label: string;
}) {
  return (
    <div className="flex rounded-lg border border-border-subtle bg-surface">
      <span className="flex h-control-md items-center gap-1.5 px-2.5 text-control text-fg-subtle">
        <Icon size="sm" />
        {label}
        <ChevronDownIcon size="sm" className="ms-1 text-fg-ghost" />
      </span>
    </div>
  );
}

export function OverviewScreen() {
  const total = VOLUME.reduce(
    (sum, p) => sum + p.error + p.warning + p.info,
    0,
  );
  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title={
          <span className="flex min-w-0 items-center gap-3">
            <PlatformIcon
              platform="node"
              size={26}
              format="lg"
              className="shrink-0 rounded-md"
            />
            <span className="truncate">{PROJECT}</span>
          </span>
        }
        summary={[
          <Text key="slug" variant="mono-sm" tone="meta">
            checkout-api
          </Text>,
          '1.5M events stored',
        ]}
        actions={
          <div className="flex gap-1.5">
            <Scope icon={EnvironmentIcon} label="Environment" />
            <Scope icon={ReleaseIcon} label="Release" />
            <Scope icon={TimeIcon} label="14d" />
          </div>
        }
      />
      <Page>
        <div className="grid grid-cols-4 gap-4">
          <Kpi
            label="Errors"
            value="302.3K"
            delta={<Delta change={12.4} />}
            footnote=" "
            trend={ERRORS.slice(-28)}
          />
          <Kpi
            label="New issues"
            value="27"
            delta={<Delta change={30.6} />}
            footnote="84 open"
          />
          <Kpi
            label="Crash-free sessions"
            value="98.86%"
            footnote="201.8K sessions"
            trend={series(28, 5, 40, 1).map((v, i) => (i > 20 ? v * 3 : v))}
          />
          <Kpi label="Crash-free users" value="99.02%" footnote="7 releases" />
        </div>
        <Panel
          title="Errors over time"
          description={`${NUMBER.format(total)} events in this window`}
          actions={
            <Button variant="ghost" size="sm" icon={CompareIcon}>
              Compare
            </Button>
          }
        >
          <BarsChart
            data={VOLUME}
            series={[
              { key: 'error', label: 'Error', color: 'var(--sev-error)' },
              { key: 'warning', label: 'Warning', color: 'var(--sev-warning)' },
              { key: 'info', label: 'Info', color: 'var(--sev-info)' },
            ]}
            xKey="t"
            height={280}
            stacked
            formatX={dayLabel}
            formatY={(v) => COMPACT.format(v)}
            label="Errors over time"
          />
        </Panel>
        <div className="grid grid-cols-5 gap-4">
          <div className="col-span-3 min-w-0">
            <Panel
              title="Open issues"
              description="What to look at first, all time"
              actions={
                <SegmentedControl aria-label="Open issues" value="recent">
                  <SegmentedItem value="recent">Latest</SegmentedItem>
                  <SegmentedItem value="new">New</SegmentedItem>
                  <SegmentedItem value="frequent">Most events</SegmentedItem>
                </SegmentedControl>
              }
            >
              <ul className="-mx-5 -mt-1">
                {ISSUES.slice(0, 5).map((issue) => (
                  <li
                    key={issue.id}
                    className="flex items-center gap-3 px-5 py-2.5"
                  >
                    <span
                      className={cn(
                        'h-8 w-0.75 shrink-0 rounded-pill',
                        LEVEL_MARK[issue.level],
                      )}
                    />
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="truncate font-medium text-fg text-value">
                        {issue.title}
                      </span>
                      <span className="truncate text-fg-subtle text-hint">
                        <span className="font-mono">{issue.id}</span> ·{' '}
                        {issue.culprit}
                      </span>
                    </span>
                    <Sparkline
                      values={issue.trend}
                      label="Events over time"
                      className="h-6 w-20 shrink-0"
                    />
                    <span className="flex w-24 shrink-0 flex-col items-end">
                      <span className="font-medium text-fg text-value tabular-nums">
                        {issue.events}
                      </span>
                      <span className="truncate text-fg-meta text-hint">
                        {issue.seen}
                      </span>
                    </span>
                  </li>
                ))}
              </ul>
            </Panel>
          </div>
          <div className="col-span-2 min-w-0">
            <Panel
              title="Session health"
              description="Crash-free sessions over the window"
            >
              <dl className="grid grid-cols-3 gap-3">
                {[
                  ['Sessions', '201.8K', 'text-fg'],
                  ['Errored', '1.9K', 'text-sev-warning-fg'],
                  ['Crashed', '2.3K', 'text-sev-error-fg'],
                ].map(([label, value, tone]) => (
                  <div key={label} className="flex min-w-0 flex-col gap-0.5">
                    <dt className="truncate text-fg-meta text-hint">{label}</dt>
                    <dd className={cn('text-section tabular-nums', tone)}>
                      {value}
                    </dd>
                  </div>
                ))}
              </dl>
            </Panel>
          </div>
        </div>
      </Page>
    </div>
  );
}
