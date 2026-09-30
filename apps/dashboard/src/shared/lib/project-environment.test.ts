import { describe, expect, it } from 'vitest';
import { projectHrefWithEnvironment } from './project-environment';

describe('project environment links', () => {
  const current =
    'https://rustrak.example/projects/7/issues?environment=staging&page=2';

  it('keeps the selection through project links and pagination', () => {
    expect(projectHrefWithEnvironment('/projects/7/releases', current)).toBe(
      '/projects/7/releases?environment=staging',
    );
    expect(projectHrefWithEnvironment('?page=3', current)).toBe(
      '/projects/7/issues?page=3&environment=staging',
    );
    expect(
      projectHrefWithEnvironment('/projects/7/issues/abc/events/def', current),
    ).toBe('/projects/7/issues/abc/events/def?environment=staging');
  });

  it('starts at all environments on project switch', () => {
    expect(projectHrefWithEnvironment('/projects/8', current)).toBe(
      '/projects/8',
    );
  });

  it('respects an explicit selection and an empty selection', () => {
    expect(
      projectHrefWithEnvironment(
        '/projects/7/logs?environment=production',
        current,
      ),
    ).toBe('/projects/7/logs?environment=production');
    expect(
      projectHrefWithEnvironment('/projects/7/logs?environment=', current),
    ).toBe('/projects/7/logs?environment=');
  });

  it('defaults to all environments when there is no selection', () => {
    expect(
      projectHrefWithEnvironment(
        '/projects/7/logs',
        'https://rustrak.example/projects/7',
      ),
    ).toBe('/projects/7/logs');
  });
});
