import type { Product, Resource } from '@rustrak/client';
import {
  AgentsIcon,
  AppearanceIcon,
  CronsIcon,
  FacetsIcon,
  FeedbackIcon,
  type IconComponent,
  InfoIcon,
  IntegrationsIcon,
  IssuesIcon,
  LinkIcon,
  LogsIcon,
  MemberIcon,
  NotificationIcon,
  OverviewIcon,
  PerformanceIcon,
  ReleasesIcon,
  SecurityIcon,
  SettingsIcon,
  StorageIcon,
  TeamIcon,
  TokensIcon,
} from '@rustrak/ui';
import { orderedProducts } from '@/features/project/model/products';
import type { Permissions } from '@/shared/lib/permissions';

/**
 * Every page the sidebar and the command bar lead to, in one table, so both
 * offer the same pages to the same reader.
 *
 * `command` is what the command bar adds to it; the label is the sidebar's
 * own, so the two cannot name a page differently. Its keys are message keys
 * resolved at render time; `keywords` is never rendered, it only widens what
 * the matcher accepts ("logout" reaches Account, "apm" reaches Performance).
 * The keywords are English-only by design: they carry platform and product
 * vocabulary, which does not get translated.
 */
export interface NavPage {
  key: string;
  icon: IconComponent;
  /** Absolute for instance pages; appended to `/projects/$id` for a project's. */
  href: string;
  /** Instance pages: shown only to who may open it; absent means everyone. */
  visible?: (p: Permissions) => boolean;
  /** Project pages: shown only to who may read this on that project. */
  reads?: Resource;
  /** The product a project page shows, when a project can turn it off. */
  product?: Product;
  command: {
    descriptionKey: string;
    keywords?: string[];
  };
}

export const canSeeTeam = (p: Permissions) =>
  p.can('users', 'read') || p.can('invitations', 'read');

export const ACCOUNT_SETTINGS: NavPage[] = [
  {
    key: 'profile',
    icon: MemberIcon,
    href: '/settings/profile',
    command: {
      descriptionKey: 'commands.accountDescription',
      keywords: ['profile', 'password', 'me', 'logout'],
    },
  },
  {
    key: 'preferences',
    icon: AppearanceIcon,
    href: '/settings/preferences',
    command: {
      descriptionKey: 'commands.appearanceDescription',
      keywords: ['theme', 'dark', 'light'],
    },
  },
];

export const WORKSPACE_SETTINGS: NavPage[] = [
  {
    key: 'team',
    icon: TeamIcon,
    href: '/settings/team',
    visible: canSeeTeam,
    command: {
      descriptionKey: 'commands.teamDescription',
      keywords: ['invite', 'people'],
    },
  },
  {
    key: 'roles',
    icon: SecurityIcon,
    href: '/settings/roles',
    visible: (p) => p.can('roles', 'read'),
    command: {
      descriptionKey: 'commands.rolesDescription',
      keywords: ['permissions', 'access', 'rbac'],
    },
  },
  {
    key: 'integrations',
    icon: IntegrationsIcon,
    href: '/settings/integrations',
    visible: (p) => p.can('integrations', 'read'),
    command: {
      descriptionKey: 'commands.integrationsDescription',
      keywords: ['slack', 'webhook', 'custom webhook', 'email'],
    },
  },
  {
    key: 'apiTokens',
    icon: TokensIcon,
    href: '/settings/tokens',
    command: {
      descriptionKey: 'commands.apiTokensDescription',
      keywords: ['auth', 'bearer', 'secret'],
    },
  },
  {
    key: 'mcp',
    icon: AgentsIcon,
    href: '/settings/mcp',
    command: {
      descriptionKey: 'commands.mcpDescription',
      keywords: ['mcp', 'ai', 'claude', 'cursor'],
    },
  },
  {
    key: 'storage',
    icon: StorageIcon,
    href: '/settings/storage',
    visible: (p) => p.can('storage', 'read'),
    command: {
      descriptionKey: 'commands.storageDescription',
      keywords: ['retention', 'cleanup', 'source maps', 'disk'],
    },
  },
  {
    key: 'instance',
    icon: InfoIcon,
    href: '/settings/instance',
    command: {
      descriptionKey: 'commands.aboutDescription',
      keywords: ['version', 'build'],
    },
  },
];

/** A project's settings pages, appended to `/projects/$id`. */
export const PROJECT_SETTINGS: NavPage[] = [
  {
    key: 'general',
    icon: SettingsIcon,
    href: '/settings/general',
    reads: 'project',
    command: {
      descriptionKey: 'commands.generalDescription',
      keywords: ['rename', 'slug', 'delete'],
    },
  },
  {
    key: 'data',
    icon: FacetsIcon,
    href: '/settings/data',
    reads: 'project',
    command: {
      descriptionKey: 'commands.dataDescription',
      keywords: ['rate limit', 'quota', 'scrubbing'],
    },
  },
  {
    key: 'clientKeys',
    icon: TokensIcon,
    href: '/settings/client-keys',
    reads: 'client-keys',
    command: {
      descriptionKey: 'commands.clientKeysDescription',
      keywords: ['dsn', 'sdk', 'setup', 'install'],
    },
  },
  {
    key: 'github',
    icon: LinkIcon,
    href: '/settings/github',
    reads: 'project-github',
    command: {
      descriptionKey: 'commands.githubDescription',
      keywords: ['github', 'repository', 'codeowners'],
    },
  },
  {
    key: 'members',
    icon: TeamIcon,
    href: '/settings/members',
    reads: 'project-members',
    command: {
      descriptionKey: 'commands.membersDescription',
      keywords: ['access', 'people'],
    },
  },
];

export const OVERVIEW: NavPage = {
  key: 'overview',
  icon: OverviewIcon,
  href: '',
  reads: 'project',
  command: {
    descriptionKey: 'commands.overviewDescription',
    keywords: ['overview', 'dashboard', 'home'],
  },
};

/** A project's pages under their sidebar headings. */
export const PROJECT_GROUPS: {
  label: 'groupTriage' | 'groupMonitor';
  pages: NavPage[];
}[] = [
  {
    label: 'groupTriage',
    pages: [
      {
        key: 'issues',
        icon: IssuesIcon,
        href: '/issues',
        reads: 'issues',
        product: 'issues',
        command: {
          descriptionKey: 'commands.issuesDescription',
          keywords: ['issues', 'errors', 'exceptions', 'crashes'],
        },
      },
      {
        key: 'feedback',
        icon: FeedbackIcon,
        href: '/feedback',
        reads: 'issues',
        product: 'feedback',
        command: {
          descriptionKey: 'commands.feedbackDescription',
          keywords: ['feedback', 'user reports', 'widget'],
        },
      },
      {
        key: 'releases',
        icon: ReleasesIcon,
        href: '/releases',
        reads: 'releases',
        product: 'releases',
        command: {
          descriptionKey: 'commands.releasesDescription',
          keywords: ['releases', 'deploys', 'versions', 'health', 'sessions'],
        },
      },
    ],
  },
  {
    label: 'groupMonitor',
    pages: [
      {
        key: 'performance',
        icon: PerformanceIcon,
        href: '/performance',
        reads: 'performance',
        product: 'performance',
        command: {
          descriptionKey: 'commands.performanceDescription',
          keywords: [
            'performance',
            'apm',
            'transactions',
            'spans',
            'traces',
            'latency',
          ],
        },
      },
      {
        key: 'logs',
        icon: LogsIcon,
        href: '/logs',
        reads: 'logs',
        product: 'logs',
        command: {
          descriptionKey: 'commands.logsDescription',
          keywords: ['logs', 'stream'],
        },
      },
      {
        key: 'crons',
        icon: CronsIcon,
        href: '/crons',
        reads: 'monitors',
        product: 'crons',
        command: {
          descriptionKey: 'commands.cronsDescription',
          keywords: [
            'crons',
            'monitors',
            'check-ins',
            'heartbeat',
            'scheduled jobs',
          ],
        },
      },
      {
        // Belongs to no product, so a project cannot turn it off.
        key: 'alerts',
        icon: NotificationIcon,
        href: '/alerts',
        reads: 'alert-rules',
        command: {
          descriptionKey: 'commands.alertsDescription',
          keywords: ['rules', 'notifications'],
        },
      },
      {
        key: 'agents',
        icon: AgentsIcon,
        href: '/agents',
        reads: 'performance',
        product: 'agents',
        command: {
          descriptionKey: 'commands.agentsDescription',
          keywords: ['agents', 'ai', 'llm', 'traces'],
        },
      },
    ],
  },
];

/**
 * A group's pages as the sidebar draws them: its products in the project's
 * saved order, then the pages no product owns.
 */
export function inProductOrder(
  pages: NavPage[],
  saved: readonly Product[] = [],
): NavPage[] {
  const rank = orderedProducts(saved);
  const of = (page: NavPage) =>
    page.product ? rank.indexOf(page.product) : rank.length;
  return [...pages].sort((a, b) => of(a) - of(b));
}

/** Every page of a project, in the sidebar's default order. */
export const PROJECT_PAGES: NavPage[] = [
  OVERVIEW,
  ...PROJECT_GROUPS.flatMap((group) => inProductOrder(group.pages)),
];

/** Whether the reader may open this page; `projectId` for a project's. */
export function allowed(
  page: NavPage,
  permissions: Permissions,
  projectId?: number,
): boolean {
  if (page.reads && projectId !== undefined) {
    return permissions.can(page.reads, 'read', projectId);
  }
  return page.visible?.(permissions) ?? true;
}

/** Whether `pathname` is `href` or a page under it. */
export const isUnder = (pathname: string, href: string) =>
  pathname === href || pathname.startsWith(`${href}/`);

/**
 * Whether the URL is a settings page: the instance's or any project's. The old
 * `settings/alerts` address, which redirects to the project's alerts page, is
 * not one: it keeps the project's sidebar while the redirect lands.
 *
 * The path alone decides. The location moves as soon as a navigation starts,
 * before the project's loader has answered, and waiting on the project made
 * the sidebar leave settings for a frame and slide back in.
 */
export function isSettingsPath(pathname: string): boolean {
  return /^\/settings(\/|$)|^\/projects\/[^/]+\/settings(?!\/alerts)(\/|$)/.test(
    pathname,
  );
}
