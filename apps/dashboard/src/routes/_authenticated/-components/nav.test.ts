import { describe, expect, it } from 'vitest';
import { NO_PERMISSIONS, permissionsFrom } from '@/shared/lib/permissions';
import { isSettingsPath, workspaceGroups } from './nav';

// Read from the path alone: during a navigation the location moves before the
// project's loader does, and a decision that waited on it flipped the sidebar
// out of settings and back.
describe('isSettingsPath', () => {
  it.each([
    '/settings',
    '/settings/profile',
    '/settings/integrations/slack',
    '/projects/3/settings',
    '/projects/3/settings/general',
    '/projects/12/settings/github',
  ])('is settings: %s', (path) => {
    expect(isSettingsPath(path)).toBe(true);
  });

  it.each([
    '/projects',
    '/projects/3',
    '/projects/3/issues',
    '/projects/3/settings/alerts',
    '/projects/3/settings/alerts/7',
    '/settingsx',
  ])('is not settings: %s', (path) => {
    expect(isSettingsPath(path)).toBe(false);
  });
});

describe('the workspace settings', () => {
  it('fall into access, connections and system, in that order', () => {
    const owner = permissionsFrom({ ...NO_PERMISSIONS, is_owner: true });
    expect(
      workspaceGroups(owner).map((g) => [g.label, g.pages.map((p) => p.key)]),
    ).toEqual([
      ['groupAccess', ['team', 'roles', 'apiTokens']],
      ['groupConnections', ['integrations', 'plugins', 'mcp']],
      ['groupSystem', ['storage', 'instance']],
    ]);
  });

  it('leave out a group with nothing the reader may open', () => {
    const nobody = permissionsFrom(NO_PERMISSIONS);
    expect(workspaceGroups(nobody).some((g) => g.pages.length === 0)).toBe(
      false,
    );
  });
});
