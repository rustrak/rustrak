import type { ProjectResponse } from '@rustrak/client';
import { FolderIcon, type IconComponent, NewIcon } from '@rustrak/ui';
import type { Permissions } from '@/shared/lib/permissions';
import {
  ACCOUNT_SETTINGS,
  allowed,
  type NavPage,
  PROJECT_PAGES,
  PROJECT_SETTINGS,
  shownInProject,
  WORKSPACE_SETTINGS,
} from '../nav';

/** A row in the command bar: a page of the nav table. */
export type CommandLink = {
  labelKey: string;
  href: string;
  descriptionKey: string;
  icon: IconComponent;
  keywords?: string[];
};

export type CommandProject = Pick<
  ProjectResponse,
  'id' | 'name' | 'platform' | 'disabled_products'
>;

/**
 * cmdk values must be unique, and names are not: `API` and `api`, or a project
 * called "All projects" beside the static row. Rows are keyed by identity and
 * stay searchable by their text through `keywords`.
 */
export const projectRow = (id: number) => `project:${id}`;
export const commandRow = (href: string) => `command:${href}`;

/** The project a row value names, or `null` for any other row. */
export function rowProject<P extends CommandProject>(
  projects: P[],
  value: string,
): P | null {
  return projects.find((project) => projectRow(project.id) === value) ?? null;
}

/** Under the label the sidebar gives it, from `labels` (its namespace). */
const asCommands = (
  pages: NavPage[],
  labels: 'settings.nav' | 'projects.nav',
): CommandLink[] =>
  pages.map((page) => ({
    ...page.command,
    labelKey: `${labels}.${page.key}`,
    href: page.href,
    icon: page.icon,
  }));

/**
 * A project's pages the reader may open and the project keeps, as `href`s
 * under the project.
 */
export function projectCommands(
  project: CommandProject,
  permissions: Permissions,
): CommandLink[] {
  const projectId = project.id;
  const shown = (page: NavPage) => shownInProject(page, permissions, project);
  return [
    ...asCommands(PROJECT_PAGES.filter(shown), 'projects.nav'),
    ...asCommands(PROJECT_SETTINGS.filter(shown), 'settings.nav'),
  ].map((link) => ({ ...link, href: `/projects/${projectId}${link.href}` }));
}

/** Commands about projects in general rather than about one of them. */
export function generalCommands(permissions: Permissions): CommandLink[] {
  return [
    {
      labelKey: 'commands.allProjects',
      href: '/projects',
      descriptionKey: 'commands.allProjectsDescription',
      icon: FolderIcon,
      keywords: ['list', 'browse'],
    },
    ...(permissions.canCreateProject
      ? [
          {
            labelKey: 'commands.newProject',
            href: '/projects/new',
            descriptionKey: 'commands.newProjectDescription',
            icon: NewIcon,
            keywords: ['create', 'add'],
          },
        ]
      : []),
  ];
}

/** The instance-wide settings the reader may open. */
export function settingsCommands(permissions: Permissions): CommandLink[] {
  return asCommands(
    [...WORKSPACE_SETTINGS, ...ACCOUNT_SETTINGS].filter((page) =>
      allowed(page, permissions),
    ),
    'settings.nav',
  );
}
