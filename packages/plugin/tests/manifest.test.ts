import { describe, expect, it } from 'vitest';
import { buildManifest, pluginId } from '../src/manifest.ts';

const pkg = {
  name: 'rustrak-plugin-hello',
  version: '1.2.3',
  description: 'Says "hi"',
  author: { name: 'Ana', email: 'ana@example.com' },
  repository: { type: 'git', url: 'https://github.com/acme/hello' },
};

describe('pluginId', () => {
  it('drops the scope and the rustrak-plugin- prefix of the package name', () => {
    expect(pluginId('rustrak-plugin-hello')).toBe('hello');
    expect(pluginId('@acme/rustrak-plugin-analytics')).toBe('analytics');
    expect(pluginId('@acme/funnels')).toBe('funnels');
  });

  it('lets the config name it explicitly', () => {
    expect(pluginId('@acme/whatever', 'stats')).toBe('stats');
  });
});

describe('buildManifest', () => {
  const toml = buildManifest({
    id: 'hello',
    pkg,
    config: {
      name: 'Hello',
      rustrak: '>=0.33.0',
      icon: 'star',
    },
  });

  it('takes identity from package.json and rustrak.config', () => {
    expect(toml).toContain('[plugin]');
    expect(toml).toContain('id = "hello"');
    expect(toml).toContain('name = "Hello"');
    expect(toml).toContain('version = "1.2.3"');
    expect(toml).toContain('description = "Says \\"hi\\""');
    expect(toml).toContain('rustrak = ">=0.33.0"');
    expect(toml).toContain('author = "Ana"');
    expect(toml).toContain('repository = "https://github.com/acme/hello"');
    expect(toml).toContain('world = "0.1.0"');
    expect(toml).toContain('icon = "star"');
  });

  it('leaves what the code declares to Rustrak, which reads it from the component', () => {
    expect(toml).not.toContain('[[routes]]');
    expect(toml).not.toContain('sql');
    // No quota of its own: no [capabilities] at all, Rustrak's default.
    expect(toml).not.toContain('[capabilities]');
  });

  it('takes the permissions from the config, as Roles lists them', () => {
    const withPermissions = buildManifest({
      id: 'hello',
      pkg,
      config: {
        name: 'Hello',
        rustrak: '>=0.33.0',
        permissions: { stats: { scope: 'project', actions: ['read'] } },
      },
    });
    expect(withPermissions).toContain(
      '[[permissions]]\nresource = "stats"\nscope = "project"\nactions = ["read"]',
    );
  });

  it('carries the storage quota and settings of the config', () => {
    const withSettings = buildManifest({
      id: 'hello',
      pkg,
      config: {
        name: 'Hello',
        rustrak: '>=0.33.0',
        storageMb: 2048,
        settings: {
          ignorePaths: { type: 'text', scope: 'project', default: '/health' },
          mode: { type: 'select', options: ['a', 'full-funnel'] },
        },
      },
    });
    expect(withSettings).toContain('storage_mb = 2048');
    expect(withSettings).toContain(
      '[[settings]]\nkey = "ignorePaths"\ntype = "text"\nscope = "project"\nrequired = false\ndefault = "/health"\n',
    );
    expect(withSettings).toContain('options = ["a", "full-funnel"]');
    expect(withSettings.indexOf('ignorePaths')).toBeLessThan(
      withSettings.indexOf('"mode"'),
    );
  });

  it('lists the pages and widgets the ui/ folder declares', () => {
    const withUi = buildManifest({
      id: 'hello',
      pkg,
      config: { name: 'Hello', rustrak: '>=0.33.0' },
      ui: {
        pages: [
          {
            path: '',
            scope: 'project',
            sidebar: { label: 'nav.hits', icon: 'link', rank: 1 },
          },
        ],
        widgets: [{ zone: 'issue.details.after' }],
      },
    });
    expect(withUi).toContain(
      '[ui]\npages = [{ path = "", scope = "project", sidebar = { label = "nav.hits", icon = "link", rank = 1 } }]\nwidgets = [{ zone = "issue.details.after" }]',
    );
    expect(toml).not.toContain('[ui]');
  });

  it('refuses an icon the design system does not have', () => {
    expect(() =>
      buildManifest({
        id: 'hello',
        pkg,
        config: {
          name: 'Hello',
          rustrak: '>=0.33.0',
          icon: 'unicorn' as never,
        },
      }),
    ).toThrow(/unicorn/);
  });
});
