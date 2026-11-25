// The UI build: ui/ folders become one script that registers the plugin with
// the dashboard (with the dashboard's own React, design system and i18n) and
// one stylesheet with only the classes the plugin uses.

import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { runInNewContext } from 'node:vm';
import * as designSystem from '@rustrak/ui';
import * as ReactQuery from '@tanstack/react-query';
import * as React from 'react';
import * as jsxRuntime from 'react/jsx-runtime';
import { renderToStaticMarkup } from 'react-dom/server';
import { IntlProvider } from 'use-intl';
import { describe, expect, it } from 'vitest';
import { readMessages } from '../src/messages.ts';
import * as ui from '../src/ui/index.tsx';
import { buildUi } from '../src/ui-build.ts';

const fixture = resolve(import.meta.dirname, 'fixtures/ui-plugin');

/** A copy of the fixture, so a test can break it without touching the others. */
function copy(edit?: (dir: string) => void): string {
  const dir = mkdtempSync(join(tmpdir(), 'rustrak-ui-src-'));
  cpSync(fixture, dir, { recursive: true });
  edit?.(dir);
  return dir;
}

async function built(dir = fixture) {
  const out = mkdtempSync(join(tmpdir(), 'rustrak-ui-'));
  const result = await buildUi(dir, 'demo', out, readMessages(dir), {
    visits: { scope: 'project', actions: ['read'] },
  });
  if (!result) throw new Error('built no UI');
  return {
    result,
    out,
    code: readFileSync(join(out, 'index.js'), 'utf8'),
  };
}

function register(code: string): ui.PluginDefinition {
  let definition: ui.PluginDefinition | undefined;
  const sandbox = {
    __rustrak: { React, jsxRuntime, ReactQuery, ui, designSystem },
    __rustrakPlugins: {
      register: (id: string, def: ui.PluginDefinition) => {
        expect(id).toBe('demo');
        definition = def;
      },
    },
  };
  runInNewContext(code, { ...sandbox, globalThis: sandbox });
  return definition!;
}

function render(
  def: ui.PluginDefinition,
  node: React.ReactElement,
  locale = 'en',
) {
  return renderToStaticMarkup(
    React.createElement(IntlProvider, {
      locale,
      messages: {},
      children: React.createElement(
        ui.PluginContext.Provider,
        {
          value: {
            pluginId: 'demo',
            projectId: 7,
            messages: def.messages,
            navigate: () => {},
            can: () => false,
          },
        },
        node,
      ),
    }),
  );
}

describe('buildUi', () => {
  it('reads pages from routes/<scope>/**/page.tsx, with their sidebar entries', async () => {
    const { result } = await built();
    expect(result.pages).toEqual([
      {
        path: '',
        scope: 'project',
        sidebar: { label: 'nav.visits', icon: 'link', rank: 1 },
      },
      { path: 'funnels/:funnelId', scope: 'project' },
      {
        path: '',
        scope: 'instance',
        sidebar: { label: 'nav.overview', icon: 'star' },
      },
    ]);
    expect(result.widgets).toEqual([{ zone: 'issue.details.after' }]);
  });

  it('leaves React and the design system to the dashboard', async () => {
    const { code } = await built();
    expect(code.length).toBeLessThan(8_000);
    expect(code).not.toContain('__CLIENT_INTERNALS');
    // @rustrak/ui is the plugin's own dependency, but the dashboard's copy
    // is the one that runs: one design system on the page.
    expect(code).toContain('__rustrak.designSystem');
  });

  it('writes a stylesheet with the classes the plugin uses, on the dashboard tokens', async () => {
    const { out } = await built();
    const css = readFileSync(join(out, 'index.css'), 'utf8');
    expect(css).toContain('.text-fg-muted');
    // The semantic token, so it follows the reader's theme.
    expect(css).toContain('var(--fg-muted)');
    // No reset: the dashboard already has one.
    expect(css).not.toMatch(/\*,\s*::after,\s*::before/);
  });

  it('registers pages, params and widgets that render with the host React', async () => {
    const def = register((await built()).code);
    const home = def.pages.find((p) => p.scope === 'project' && p.path === '')!;
    expect(
      render(
        def,
        React.createElement(home.component, { projectId: 7, params: {} }),
      ),
    ).toContain('project 7');
    const funnel = def.pages.find((p) => p.path === 'funnels/:funnelId')!;
    expect(
      render(
        def,
        React.createElement(funnel.component, {
          projectId: 7,
          params: { funnelId: '42' },
        }),
      ),
    ).toContain('funnel 42');
    const widget = def.widgets[0]!;
    expect(widget.zone).toBe('issue.details.after');
    expect(
      render(
        def,
        React.createElement(widget.component, {
          projectId: 7,
          issue: { id: 'abc', title: 'Boom' },
        }),
      ),
    ).toBe('<span>abc:3</span>');
  });

  it("speaks the reader's language, falling back to English key by key", async () => {
    const def = register((await built()).code);
    const home = def.pages.find((p) => p.scope === 'project' && p.path === '')!;
    const page = (locale: string) =>
      render(
        def,
        React.createElement(home.component, { projectId: 7, params: {} }),
        locale,
      );
    expect(page('es')).toContain('Hola Ana');
    expect(page('es')).toContain('Visitas');
    expect(page('fr')).toContain('Hello Ana');
  });

  it('builds a page that uses the design system at its top level', async () => {
    const dir = copy((d) => {
      const page = join(d, 'src/ui/routes/project/page.tsx');
      writeFileSync(
        page,
        `import { createDataTableColumnHelper } from '@rustrak/ui';\nconst column = createDataTableColumnHelper<{ id: string }>();\nexport const columns = column.columns([column.accessor('id', {})]);\n${readFileSync(page, 'utf8')}`,
      );
    });
    const { result } = await built(dir);
    expect(result.pages.map((p) => p.path)).toContain('');
  });

  it('carries the permission a page requires into the manifest', async () => {
    const dir = copy((d) => {
      const page = join(d, 'src/ui/routes/project/page.tsx');
      writeFileSync(
        page,
        readFileSync(page, 'utf8').replace(
          'sidebar: {',
          "requires: 'visits:read', sidebar: {",
        ),
      );
    });
    const { result } = await built(dir);
    expect(result.pages.find((p) => p.path === '')?.requires).toBe(
      'visits:read',
    );
  });

  it('refuses a page that requires a permission the config does not declare', async () => {
    const dir = copy((d) => {
      const page = join(d, 'src/ui/routes/project/page.tsx');
      writeFileSync(
        page,
        readFileSync(page, 'utf8').replace(
          'sidebar: {',
          "requires: 'secrets:read' as never, sidebar: {",
        ),
      );
    });
    await expect(built(dir)).rejects.toThrow(
      /page\.tsx: requires secrets:read, which rustrak\.config\.ts does not declare/,
    );
  });

  it('refuses a sidebar label that is not a message', async () => {
    const dir = copy((d) => {
      const page = join(d, 'src/ui/routes/project/page.tsx');
      writeFileSync(
        page,
        readFileSync(page, 'utf8').replace(
          "'nav.visits', icon",
          "'nav.nope', icon",
        ),
      );
    });
    await expect(built(dir)).rejects.toThrow(/nav\.nope/);
  });

  it('refuses an icon the design system does not have', async () => {
    const dir = copy((d) => {
      const page = join(d, 'src/ui/routes/instance/page.tsx');
      writeFileSync(
        page,
        readFileSync(page, 'utf8').replace("icon: 'star'", "icon: 'unicorn'"),
      );
    });
    await expect(built(dir)).rejects.toThrow(/unicorn/);
  });

  it("draws into its own setup, per project, in the plugin's settings", async () => {
    const dir = copy((d) => {
      const widget = join(d, 'src/ui/widgets/issue-visits.tsx');
      writeFileSync(
        widget,
        readFileSync(widget, 'utf8').replace(
          'issue.details.after',
          'plugin.setup',
        ),
      );
    });
    const { result } = await built(dir);
    expect(result.widgets).toEqual([{ zone: 'plugin.setup' }]);
  });

  it('refuses a widget that names no zone', async () => {
    const dir = copy((d) =>
      writeFileSync(
        join(d, 'src/ui/widgets/issue-visits.tsx'),
        'export default () => null;',
      ),
    );
    await expect(built(dir)).rejects.toThrow(/names its zone/);
  });

  it('refuses a page outside project/ or instance/', async () => {
    const dir = copy((d) => {
      mkdirSync(join(d, 'src/ui/routes/admin'), { recursive: true });
      writeFileSync(
        join(d, 'src/ui/routes/admin/page.tsx'),
        'export default () => null;',
      );
    });
    await expect(built(dir)).rejects.toThrow(/routes\/admin/);
  });

  it('refuses a settings page: Rustrak draws it from rustrak.config.ts', async () => {
    const dir = copy((d) => {
      mkdirSync(join(d, 'src/ui/routes/settings'), { recursive: true });
      writeFileSync(
        join(d, 'src/ui/routes/settings/page.tsx'),
        'export default () => null;',
      );
    });
    await expect(built(dir)).rejects.toThrow(/drawn from rustrak\.config\.ts/);
  });

  it('builds nothing for a plugin without ui/', async () => {
    const dir = copy((d) => rmSync(join(d, 'src', 'ui'), { recursive: true }));
    const out = mkdtempSync(join(tmpdir(), 'rustrak-ui-'));
    expect(await buildUi(dir, 'demo', out, {})).toBeUndefined();
    expect(existsSync(join(out, 'index.js'))).toBe(false);
  });
});
