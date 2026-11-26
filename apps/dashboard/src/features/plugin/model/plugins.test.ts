import type { PluginSummary, RegistryPlugin } from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import {
  activeIn,
  catalog,
  catalogSearch,
  matchPage,
  pageAllowed,
  pageAt,
  pageHref,
  readmeIn,
  sectionOf,
  viewPage,
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

  it('prefers a fixed segment over a parameter, whatever the order', () => {
    const sorted = [
      { path: 'funnels/:id', scope: 'project' },
      { path: 'funnels/new', scope: 'project' },
    ];
    expect(matchPage(sorted, 'project', 'funnels/new')?.page).toBe(sorted[1]);
    expect(matchPage(sorted, 'project', 'funnels/7')?.page).toBe(sorted[0]);
  });

  it('decodes a parameter once: the router already decoded the address', () => {
    expect(matchPage(pages, 'project', 'funnels/100%25')?.params.funnelId).toBe(
      '100%25',
    );
  });

  it('finds nothing for an address no page has', () => {
    expect(matchPage(pages, 'project', 'funnels/42/steps')).toBeNull();
    expect(matchPage(pages, 'settings', '')).toBeNull();
  });
});

describe('viewPage', () => {
  it('is the address itself when it fits', () => {
    expect(viewPage('hello', 'funnels/42')).toBe('plugin:hello/funnels/42');
  });

  it('keeps long addresses apart within 64 characters', () => {
    const a = viewPage('hello', `${'x'.repeat(80)}/a`);
    const b = viewPage('hello', `${'x'.repeat(80)}/b`);
    expect(a.length).toBeLessThanOrEqual(64);
    expect(b.length).toBeLessThanOrEqual(64);
    expect(a).not.toBe(b);
    expect(a.startsWith('plugin~hello/xxx')).toBe(true);
    expect(viewPage('hello', `${'x'.repeat(80)}/a`)).toBe(a);
  });

  it('never gives a short address the key of a long one', () => {
    const long = viewPage('hello', 'x'.repeat(80));
    // The rest of a hashed key, read back as a short address of its own.
    const short = viewPage('hello', long.slice('plugin:hello/'.length));
    expect(short).not.toBe(long);
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
  const traffic = plugin({
    id: 'web-traffic',
    permissions: [
      { resource: 'traffic', actions: ['read'], scope: 'instance' },
      { resource: 'visitors', actions: ['read'], scope: 'project' },
    ],
  });
  const can = (key: string, action: string) =>
    key === 'web-traffic:traffic' && action === 'read';

  it('is any page that requires nothing, else one whose permission is held', () => {
    expect(pageAllowed({}, traffic, can)).toBe(true);
    expect(pageAllowed({ requires: 'traffic:read' }, traffic, can)).toBe(true);
    expect(pageAllowed({ requires: 'visitors:read' }, traffic, can)).toBe(
      false,
    );
  });

  it("asks with the permission's scope", () => {
    const scopes: unknown[] = [];
    const record = (_k: string, _a: string, _p?: number, scope?: string) =>
      scopes.push(scope) > 0;
    pageAllowed({ requires: 'traffic:read' }, traffic, record, 3);
    pageAllowed({ requires: 'visitors:read' }, traffic, record, 3);
    expect(scopes).toEqual(['instance', 'project']);
  });
});

describe('catalog', () => {
  const published = (
    over: Partial<RegistryPlugin> & Pick<RegistryPlugin, 'id'>,
  ): RegistryPlugin => ({
    name: over.id,
    description: null,
    author: null,
    icon: null,
    logo: null,
    version: '1.0.0',
    installed_version: null,
    ...over,
  });
  const installed = [
    plugin({ id: 'web-traffic', name: 'Web Traffic', verified: true }),
    plugin({ id: 'mine', name: 'Mine', description: 'My own uptime checks' }),
  ];
  const registry = [
    published({
      id: 'web-traffic',
      installed_version: '0.1.0',
      version: '0.2.0',
    }),
    published({ id: 'uptime', name: 'Uptime' }),
  ];
  const ids = (filter: Parameters<typeof catalog>[2], query = '') =>
    catalog(installed, registry, filter, query).map((e) => e.id);

  it('lists installed and published plugins once each, by name', () => {
    expect(ids('all')).toEqual(['mine', 'uptime', 'web-traffic']);
    const traffic = catalog(installed, registry, 'all', '').at(-1);
    expect(traffic?.installed?.id).toBe('web-traffic');
    expect(traffic?.published?.version).toBe('0.2.0');
  });

  it('filters by what is installed, available or has an update', () => {
    expect(ids('installed')).toEqual(['mine', 'web-traffic']);
    expect(ids('available')).toEqual(['uptime']);
    expect(ids('updates')).toEqual(['web-traffic']);
  });

  it('searches name, id and description, any case', () => {
    expect(ids('all', 'UPTIME')).toEqual(['mine', 'uptime']);
    expect(ids('all', 'traffic')).toEqual(['web-traffic']);
  });

  it('lists the installed ones alone when the registry is not read', () => {
    expect(catalog(installed, undefined, 'all', '').map((e) => e.id)).toEqual([
      'mine',
      'web-traffic',
    ]);
  });
});

describe('catalogSearch', () => {
  it('keeps a search and a known filter, and drops the rest', () => {
    expect(catalogSearch({ q: 'traffic', filter: 'updates' })).toEqual({
      q: 'traffic',
      filter: 'updates',
    });
    expect(catalogSearch({ q: '', filter: 'all' })).toEqual({
      q: undefined,
      filter: undefined,
    });
    expect(catalogSearch({ q: 3, filter: 'nope' })).toEqual({
      q: undefined,
      filter: undefined,
    });
  });
});
