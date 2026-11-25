import type { PluginSummary } from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import {
  activeIn,
  matchPage,
  pageAllowed,
  pageAt,
  pageHref,
  readmeIn,
  sectionOf,
} from './plugins';

const plugin = (over: Partial<PluginSummary>): PluginSummary => ({
  id: 'hello',
  name: 'Hello',
  version: '0.1.0',
  description: null,
  author: null,
  repository: null,
  icon: null,
  readme: { en: '' },
  database: false,
  permissions: [],
  allProjects: false,
  projects: [],
  ui: null,
  verified: false,
  logo: null,
  settings: [],
  messages: {},
  ...over,
});

describe('activeIn', () => {
  it('is everywhere or where it was chosen', () => {
    expect(activeIn(plugin({ allProjects: true }), 9)).toBe(true);
    expect(activeIn(plugin({ projects: [3] }), 3)).toBe(true);
    expect(activeIn(plugin({ projects: [3] }), 9)).toBe(false);
  });
});

describe('matchPage', () => {
  const pages = [
    { path: '', scope: 'project' as const },
    { path: 'funnels', scope: 'project' as const },
    { path: 'funnels/:funnelId', scope: 'project' as const },
    { path: '', scope: 'instance' as const },
  ];

  it('finds the page an address names, with its parameters', () => {
    expect(matchPage(pages, 'project', pageAt(undefined))).toEqual({
      page: pages[0],
      params: {},
    });
    expect(matchPage(pages, 'project', pageAt('funnels/42/'))).toEqual({
      page: pages[2],
      params: { funnelId: '42' },
    });
    expect(matchPage(pages, 'instance', '')?.page).toBe(pages[3]);
  });

  it('finds nothing for an address no page has', () => {
    expect(matchPage(pages, 'project', 'funnels/42/steps')).toBeNull();
    expect(matchPage(pages, 'settings', '')).toBeNull();
  });
});

describe('pageHref', () => {
  it('writes a page address in a project or outside any', () => {
    expect(pageHref('hello', '', 3)).toBe('/projects/3/plugins/hello');
    expect(pageHref('hello', 'about', 3)).toBe(
      '/projects/3/plugins/hello/about',
    );
    expect(pageHref('hello', '')).toBe('/plugins/hello');
  });
});

describe('sectionOf', () => {
  const pages = [
    { path: '', sidebar: { label: 'nav.traffic' } },
    { path: 'visitors', sidebar: { label: 'nav.visitors' } },
    { path: 'visitors/:visitorId' },
    { path: 'breakdown/:by' },
  ];

  it('is the sidebar page a page sits under, the deepest one', () => {
    expect(sectionOf(pages, 'visitors/:visitorId')?.path).toBe('visitors');
    expect(sectionOf(pages, 'visitors')?.path).toBe('visitors');
    expect(sectionOf(pages, 'breakdown/:by')?.path).toBe('');
  });

  it('is not a page whose path only starts with the same letters', () => {
    expect(
      sectionOf([{ path: 'visit', sidebar: { label: 'x' } }], 'visitors')?.path,
    ).toBeUndefined();
  });
});

describe('readmeIn', () => {
  const readme = { en: '# Hello', es: '# Hola', 'pt-BR': '# Olá' };

  it("is the reader's language, then its base, then English", () => {
    expect(readmeIn(readme, 'es')).toBe('# Hola');
    expect(readmeIn(readme, 'es-ES')).toBe('# Hola');
    expect(readmeIn(readme, 'pt-BR')).toBe('# Olá');
    expect(readmeIn(readme, 'fr')).toBe('# Hello');
    expect(readmeIn({}, 'fr')).toBe('');
  });
});

describe('pageAllowed', () => {
  const can = (key: string, action: string) =>
    key === 'web-traffic:traffic' && action === 'read';

  it('is any page that requires nothing, else one whose permission is held', () => {
    expect(pageAllowed({}, 'web-traffic', can)).toBe(true);
    expect(pageAllowed({ requires: 'traffic:read' }, 'web-traffic', can)).toBe(
      true,
    );
    expect(pageAllowed({ requires: 'visitors:read' }, 'web-traffic', can)).toBe(
      false,
    );
  });
});
