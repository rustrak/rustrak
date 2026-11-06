import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { withoutComments } from './predicates';

/**
 * What a reader's permissions do to a page, which only shows with a role that
 * has some permissions and not others: an owner never sees any of this.
 *
 * 1. **A form opens only for who may submit it.** Otherwise a direct link is a
 *    form that fills in and fails on submit.
 * 2. **A page does not suspend on another resource's read.** A 403 thrown from
 *    `useSuspenseQuery` takes the page down, so an issue would refuse to open
 *    for lack of a permission about GitHub.
 */
const here = dirname(fileURLToPath(import.meta.url));

function route(relative: string): string {
  const path = resolve(here, '../../routes/_authenticated', relative);
  return withoutComments(readFileSync(path, 'utf8'));
}

describe('a form opens only for who may submit it', () => {
  it.each([
    ['projects/new.tsx', /canCreateProject/],
    ['projects/$id/crons/new.tsx', /can\('monitors', 'create', /],
    ['projects/$id/alerts/new.tsx', /can\('alert-rules', 'create', /],
    ['projects/$id/alerts/$ruleId/edit.tsx', /can\('alert-rules', 'update', /],
    ['projects/$id/settings/members.tsx', /can\('project-members', 'read', /],
  ])('%s', (file, gate) => {
    expect(route(file)).toMatch(gate);
    expect(route(file)).toMatch(/NotAuthorized/);
  });
});

describe('a page does not suspend on another resource', () => {
  it.each([
    [
      'projects/$id/issues/$issueId/events/$eventId.tsx',
      /useSuspenseQuery\(reads\.repository\)/,
    ],
    [
      'projects/$id/alerts/new.tsx',
      /useSuspenseQuery\(\s*projectQueries\.environments/,
    ],
    [
      'projects/$id/alerts/$ruleId/edit.tsx',
      /useSuspenseQuery\(\s*projectQueries\.environments/,
    ],
  ])('%s', (file, suspends) => {
    expect(route(file)).not.toMatch(suspends);
  });
});

describe('an action is offered only to who may take it', () => {
  it('an integration form offers delete only with integrations:delete', () => {
    const settings = withoutComments(
      readFileSync(
        resolve(
          here,
          '../../features/alert/ui/components/provider-settings.tsx',
        ),
        'utf8',
      ),
    );
    expect(settings).toMatch(
      /onDelete=\{can\.delete \? setDeleting : undefined\}/,
    );
  });
});

describe('every permission has somewhere to be used', () => {
  it('project members have their own page, not a redirect', () => {
    expect(route('projects/$id/settings/members.tsx')).not.toMatch(/redirect/);
  });

  it('the team page opens for who only handles invitations', () => {
    const page = route('settings/team/index.tsx');
    expect(page).toMatch(/can\('invitations', 'create'\)/);
    expect(page).not.toMatch(
      /canInvite && permissions\.can\('users', 'read'\)/,
    );
  });

  it('a new API token is offered only with the permission', () => {
    expect(route('settings/tokens.tsx')).toMatch(
      /can\('api-tokens', 'create'\)/,
    );
  });
});
