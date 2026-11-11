import { describe, expect, it } from 'vitest';
import { permissionsFrom } from '@/shared/lib/permissions';
import {
  ACCOUNT_SETTINGS,
  PROJECT_PAGES,
  PROJECT_SETTINGS,
  WORKSPACE_SETTINGS,
} from '../nav';
import {
  type CommandProject,
  projectCommands,
  projectRow,
  rowProject,
  settingsCommands,
} from './commands';

const owner = permissionsFrom({
  is_owner: true,
  instance: [],
  all_projects: [],
  projects: [],
});

const project = (
  id: number,
  name: string,
  disabled_products: CommandProject['disabled_products'] = [],
): CommandProject => ({ id, name, platform: null, disabled_products });

// The bar and the sidebar read one table: every page the sidebar lists is a
// command, under the sidebar's own words.
describe('commands', () => {
  it('offers every settings page under its sidebar label', () => {
    expect(settingsCommands(owner).map((c) => c.labelKey)).toEqual(
      [...WORKSPACE_SETTINGS, ...ACCOUNT_SETTINGS].map(
        (p) => `settings.nav.${p.key}`,
      ),
    );
  });

  it('offers every project page under its sidebar label', () => {
    expect(
      projectCommands(project(3, 'acme'), owner).map((c) => c.labelKey),
    ).toEqual([
      ...PROJECT_PAGES.map((p) => `projects.nav.${p.key}`),
      ...PROJECT_SETTINGS.map((p) => `settings.nav.${p.key}`),
    ]);
  });

  it('lists project pages in the order the sidebar draws them', () => {
    const keys = projectCommands(project(3, 'acme'), owner).map(
      (c) => c.labelKey,
    );
    // Pages no product owns (alerts) come after a group's products.
    expect(keys.indexOf('projects.nav.agents')).toBeLessThan(
      keys.indexOf('projects.nav.alerts'),
    );
  });

  it('leaves out the pages of products the project turned off', () => {
    const hrefs = projectCommands(
      project(3, 'acme', ['performance', 'logs']),
      owner,
    ).map((c) => c.href);
    expect(hrefs).not.toContain('/projects/3/performance');
    expect(hrefs).not.toContain('/projects/3/logs');
    // Agents need performance too, the same rule as the sidebar.
    expect(hrefs).not.toContain('/projects/3/agents');
    expect(hrefs).toContain('/projects/3/issues');
  });
});

// cmdk needs a unique value per row; names are neither unique nor distinct
// from the static rows, so a row is resolved back to its project by id.
describe('rowProject', () => {
  const projects = [
    project(1, 'API'),
    project(2, 'api'),
    project(3, 'All projects'),
  ];

  it('finds the project a row names, by id', () => {
    expect(rowProject(projects, projectRow(2))?.id).toBe(2);
    expect(rowProject(projects, projectRow(1))?.id).toBe(1);
  });

  it('finds none for a row that is not a project', () => {
    expect(rowProject(projects, 'command:/projects')).toBeNull();
    expect(rowProject(projects, 'All projects')).toBeNull();
  });
});
