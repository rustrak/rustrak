/**
 * The documentation's navigation: four tabs, each scoping the sidebar to the
 * part of the docs a reader came for, as Bun, Astro and Clerk do. Running
 * Rustrak and using it are different readers on different days; neither
 * needs the other's forty links in the way.
 *
 * A page's slug is its path under `content/`.
 */

/**
 * A page, by its path under `content/`. One with an `href` is not a docs page
 * but sits in the sidebar all the same: the interactive API reference.
 */
export type NavItem = { slug: string; title: string; href?: string };
export type NavGroup = { title: string; items: NavItem[] };
export type NavTab = {
  id: string;
  title: string;
  /** A word beside the title, for a section that is new or not final: `Beta`. */
  badge?: string;
  /** Where the tab lands: its first page. */
  href: string;
  groups: NavGroup[];
};

const item = (slug: string, title: string): NavItem => ({ slug, title });

export const TABS: NavTab[] = [
  {
    id: 'guides',
    title: 'Guides',
    href: '/getting-started/overview',
    groups: [
      {
        title: 'Getting started',
        items: [
          item('getting-started/overview', 'Overview'),
          item('getting-started/installation', 'Installation'),
          item('getting-started/quickstart', 'Quickstart'),
        ],
      },
      {
        title: 'Triage',
        items: [
          item('usage/projects', 'Projects'),
          item('usage/issues', 'Issues'),
          item('usage/saved-views', 'Saved views'),
          item('usage/user-feedback', 'User feedback'),
          item('usage/releases', 'Releases'),
        ],
      },
      {
        title: 'Monitor',
        items: [
          item('usage/performance', 'Performance'),
          item('usage/web-vitals', 'Web Vitals'),
          item('usage/logs', 'Logs'),
          item('usage/crons', 'Crons'),
          item('usage/agents', 'AI agents'),
          item('usage/alerts', 'Alerts'),
          item('usage/alerts/message-body-template', 'Message body template'),
        ],
      },
      {
        title: 'Symbolication',
        items: [
          item('usage/source-maps', 'Source maps'),
          item('usage/debug-files', 'Debug files'),
        ],
      },
      {
        title: 'Organization',
        items: [
          item('usage/team', 'Team & access'),
          item('usage/roles', 'Roles & permissions'),
          item('usage/tokens', 'API tokens'),
          item('usage/storage', 'Storage'),
          item('usage/language-and-region', 'Language & region'),
        ],
      },
    ],
  },
  {
    id: 'self-hosting',
    title: 'Self-hosting',
    href: '/configuration/environment',
    groups: [
      {
        title: 'Configuration',
        items: [
          item('configuration/environment', 'Environment variables'),
          item('configuration/database', 'Database backends'),
          item('configuration/bootstrap-file', 'Bootstrap file'),
          item('configuration/production', 'Production checklist'),
          item('configuration/telemetry', 'Telemetry'),
        ],
      },
      {
        title: 'Integrations',
        items: [
          item('configuration/sso', 'Single sign-on'),
          item('configuration/github', 'GitHub'),
          item('configuration/gitlab', 'GitLab'),
        ],
      },
      {
        title: 'Upgrading',
        items: [
          item('upgrading', 'Overview'),
          item('upgrading/0-25', 'Upgrade to 0.25'),
          item('upgrading/0-15', 'Upgrade to 0.15'),
        ],
      },
      {
        title: 'Troubleshooting',
        items: [
          item('troubleshooting/common-issues', 'Common issues'),
          item('troubleshooting/faq', 'FAQ'),
        ],
      },
    ],
  },
  {
    id: 'plugins',
    title: 'Plugins',
    badge: 'BETA',
    href: '/plugins/overview',
    groups: [
      {
        title: 'Plugins (beta)',
        items: [
          item('plugins/overview', 'Overview'),
          item('plugins/install', 'Install and manage'),
        ],
      },
      {
        title: 'Build a plugin',
        items: [
          item('plugins/create', 'Create a plugin'),
          item('plugins/configuration', 'Configuration'),
          item('plugins/server', 'Server routes'),
          item('plugins/database', 'Database'),
          item('plugins/ui', 'Pages and widgets'),
          item('plugins/publish', 'Build and publish'),
        ],
      },
    ],
  },
  {
    id: 'developers',
    title: 'Developers',
    href: '/reference/api',
    groups: [
      {
        title: 'API',
        items: [
          item('reference/api', 'REST API'),
          {
            slug: 'api-reference',
            title: 'Interactive reference',
            href: '/api-reference',
          },
          item('sdks/client', '@rustrak/client'),
          item('sdks/cli', 'rustrak CLI'),
        ],
      },
      {
        title: 'AI assistants',
        items: [
          item('sdks/mcp', 'MCP server'),
          item('reference/llms', 'Docs for AI assistants'),
        ],
      },
      {
        title: 'Project',
        items: [
          item('reference/architecture', 'Architecture'),
          item('reference/contributing', 'Contributing'),
        ],
      },
    ],
  },
];

/** Every page in reading order, with where it sits: for the pager and breadcrumbs. */
export const PAGES = TABS.flatMap((tab) =>
  tab.groups.flatMap((group) =>
    group.items.map((page) => ({ ...page, tab, group })),
  ),
);

export const findPage = (slug: string) => PAGES.find((p) => p.slug === slug);

/** Where a sidebar entry or the pager goes. */
export const hrefOf = (page: NavItem) => page.href ?? `/${page.slug}`;
