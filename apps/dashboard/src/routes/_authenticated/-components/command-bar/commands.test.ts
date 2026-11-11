import { describe, expect, it } from 'vitest';
import { permissionsFrom } from '@/shared/lib/permissions';
import {
  ACCOUNT_SETTINGS,
  PROJECT_PAGES,
  PROJECT_SETTINGS,
  WORKSPACE_SETTINGS,
} from '../nav';
import { projectCommands, settingsCommands } from './commands';

const owner = permissionsFrom({
  is_owner: true,
  instance: [],
  all_projects: [],
  projects: [],
});

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
    expect(projectCommands(3, owner).map((c) => c.labelKey)).toEqual([
      ...PROJECT_PAGES.map((p) => `projects.nav.${p.key}`),
      ...PROJECT_SETTINGS.map((p) => `settings.nav.${p.key}`),
    ]);
  });

  it('lists project pages in the order the sidebar draws them', () => {
    const keys = projectCommands(3, owner).map((c) => c.labelKey);
    // Pages no product owns (alerts) come after a group's products.
    expect(keys.indexOf('projects.nav.agents')).toBeLessThan(
      keys.indexOf('projects.nav.alerts'),
    );
  });
});
