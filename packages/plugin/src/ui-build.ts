// The UI half of `rustrak-plugin build`. Folders are the API:
//
//   src/ui/routes/project/page.tsx                    → the plugin's page in each project
//   src/ui/routes/project/funnels/[funnelId]/page.tsx → a page with a parameter
//   src/ui/routes/instance/page.tsx                   → a page outside any project
//   src/ui/widgets/<name>.tsx                         → `defineWidget({ zone })`
//   src/messages/<locale>.json                        → its texts; en.json is required
//
// Any other file is an ordinary module (components, hooks, helpers). A page
// goes in the sidebar with `export const config = definePage({ sidebar })`.
//
// Everything becomes one script that registers the plugin with the dashboard
// and one stylesheet with only the classes the plugin uses, built on the
// dashboard's own tokens. React, the query cache, the design system and the
// translator are the dashboard's.

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join, relative, resolve, sep } from 'node:path';
import { runInNewContext } from 'node:vm';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { build } from 'vite';
import { filesIn, type Permissions, paths, type Scope } from './config.ts';
import { checkIcon } from './manifest.ts';
import { keys, type Messages } from './messages.ts';
import type { PageConfig, WidgetConfig } from './ui/index.tsx';

/** Pages are listed project ones first. */
const SCOPES: readonly Scope[] = ['project', 'instance'];

export interface UiPage extends PageConfig {
  path: string;
  scope: Scope;
}

export interface UiManifest {
  pages: UiPage[];
  widgets: WidgetConfig[];
}

/** The dashboard's modules a plugin uses without bundling them. */
const SHARED: Record<string, string> = {
  react: '__rustrak.React',
  'react/jsx-runtime': '__rustrak.jsxRuntime',
  'react-dom': '__rustrak.ReactDOM',
  '@tanstack/react-query': '__rustrak.ReactQuery',
  '@rustrak/plugin/ui': '__rustrak.ui',
  '@rustrak/ui': '__rustrak.designSystem',
};

const SOURCE = /\.(tsx|jsx|ts|js)$/;
const PAGE = /^page\.(tsx|jsx)$/;
const posix = (path: string) => path.split(sep).join('/');

/** `funnels/[funnelId]/page.tsx` → `funnels/:funnelId`. */
function routePath(dirs: string[], file: string): string {
  return dirs
    .map((seg) => {
      const param = /^\[([a-zA-Z][a-zA-Z0-9]*)\]$/.exec(seg)?.[1];
      if (param) return `:${param}`;
      if (!/^[a-z0-9-]+$/.test(seg)) {
        throw new Error(
          `${file}: a folder is lowercase letters, digits and dashes, or [param]`,
        );
      }
      return seg;
    })
    .join('/');
}

/**
 * Builds `projectDir`'s src/ui/ into `outDir/index.js` and `index.css`;
 * nothing, and `undefined`, for a plugin without pages or widgets.
 */
export async function buildUi(
  projectDir: string,
  id: string,
  outDir: string,
  messages: Record<string, Messages>,
  permissions: Permissions = {},
): Promise<UiManifest | undefined> {
  const dir = resolve(projectDir);
  const where = paths(dir);
  const routes = join(where.ui, 'routes');

  const pages = filesIn(routes)
    .filter((file) => PAGE.test(file.slice(file.lastIndexOf(sep) + 1)))
    .map((file) => {
      const shown = `src/ui/routes/${posix(relative(routes, file))}`;
      const [scope, ...dirs] = posix(relative(routes, file))
        .split('/')
        .slice(0, -1);
      if (!SCOPES.includes(scope as Scope)) {
        throw new Error(
          `${shown}: pages live in src/ui/routes/project or src/ui/routes/instance (a plugin's settings page is Rustrak's, drawn from rustrak.config.ts)`,
        );
      }
      return {
        scope: scope as Scope,
        path: routePath(dirs, shown),
        file,
        shown,
      };
    });
  pages.sort(
    (a, b) =>
      SCOPES.indexOf(a.scope) - SCOPES.indexOf(b.scope) ||
      a.path.localeCompare(b.path),
  );
  const widgets = filesIn(join(where.ui, 'widgets'))
    .filter((file) => SOURCE.test(file))
    .map((file) => ({
      file,
      shown: `src/ui/${posix(relative(where.ui, file))}`,
    }));

  if (pages.length === 0 && widgets.length === 0) return undefined;

  const known = new Set(keys(messages.en ?? {}));

  const work = where.generated;
  mkdirSync(work, { recursive: true });
  const rel = (file: string) => `./${posix(relative(work, file))}`;
  const locales = Object.keys(messages);
  const list = (items: string[]) => items.join(', ');
  writeFileSync(
    join(work, 'ui-entry.jsx'),
    [
      "import './ui.css';",
      ...pages.map(
        (p, i) => `import * as page${i} from ${JSON.stringify(rel(p.file))};`,
      ),
      ...widgets.map(
        (w, i) => `import * as widget${i} from ${JSON.stringify(rel(w.file))};`,
      ),
      ...locales.map(
        (locale, i) =>
          `import messages${i} from ${JSON.stringify(rel(join(where.messages, `${locale}.json`)))};`,
      ),
      // Read through a parameter: a page or widget may leave `config` out.
      'const page = (m, path, scope, file) => ({ path, scope, file, config: m.config, component: m.default });',
      'const widget = (m, file) => ({ zone: m.config && m.config.zone, file, component: m.default });',
      `globalThis.__rustrakPlugins.register(${JSON.stringify(id)}, {`,
      `  pages: [${list(pages.map((p, i) => `page(page${i}, ${JSON.stringify(p.path)}, ${JSON.stringify(p.scope)}, ${JSON.stringify(p.shown)})`))}],`,
      `  widgets: [${list(widgets.map((w, i) => `widget(widget${i}, ${JSON.stringify(w.shown)})`))}],`,
      `  messages: { ${list(locales.map((l, i) => `${JSON.stringify(l)}: messages${i}`))} },`,
      '});',
      '',
    ].join('\n'),
  );

  // The dashboard's tokens and Tailwind's utilities, without its reset: the
  // dashboard already has one. Resolved from this package, so a plugin needs
  // neither installed.
  const from = createRequire(import.meta.url);
  writeFileSync(
    join(work, 'ui.css'),
    [
      '@layer theme, base, components, utilities;',
      `@import ${JSON.stringify(from.resolve('tailwindcss/theme.css'))} layer(theme);`,
      `@import ${JSON.stringify(from.resolve('tailwindcss/utilities.css'))} layer(utilities);`,
      `@import ${JSON.stringify(from.resolve('@rustrak/ui/tokens.css'))};`,
      `@source ${JSON.stringify(where.ui)};`,
      '',
    ].join('\n'),
  );

  // Production JSX whatever the caller's NODE_ENV: Vite reads it to decide.
  const env = process.env.NODE_ENV;
  process.env.NODE_ENV = 'production';
  try {
    await bundle(dir, join(work, 'ui-entry.jsx'), outDir);
  } finally {
    if (env === undefined) delete process.env.NODE_ENV;
    else process.env.NODE_ENV = env;
  }

  const registered = describe(readFileSync(join(outDir, 'index.js'), 'utf8'));
  const granted = new Set(
    Object.entries(permissions).flatMap(([resource, p]) =>
      p.actions.map((a) => `${resource}:${a}`),
    ),
  );
  const manifestPages = registered.pages.map((p) =>
    manifestPage(p, known, granted),
  );
  // Which zones exist, `Zone` types; Rustrak checks it again at install.
  const manifestWidgets = registered.widgets.map((w) => {
    if (!w.zone) {
      throw new Error(
        `${w.file}: a widget names its zone: export const config = defineWidget({ zone })`,
      );
    }
    return { zone: w.zone };
  });
  return { pages: manifestPages, widgets: manifestWidgets };
}

/** A registered page as the manifest lists it, its declarations checked. */
function manifestPage(
  p: Registered['pages'][number],
  known: Set<string>,
  granted: Set<string>,
): UiPage {
  const sidebar = p.config?.sidebar;
  if (sidebar && !known.has(sidebar.label)) {
    throw new Error(
      `${p.file}: sidebar label ${sidebar.label} is not a key of src/messages/en.json`,
    );
  }
  checkIcon(sidebar?.icon, p.file);
  const requires = p.config?.requires;
  if (requires && !granted.has(requires)) {
    throw new Error(
      `${p.file}: requires ${requires}, which rustrak.config.ts does not declare in permissions`,
    );
  }
  return {
    path: p.path,
    scope: p.scope,
    ...(sidebar ? { sidebar: clean(sidebar) } : {}),
    ...(requires ? { requires } : {}),
  };
}

function clean<T extends object>(value: T): T {
  return Object.fromEntries(
    Object.entries(value).filter(([, v]) => v !== undefined),
  ) as T;
}

interface Registered {
  pages: {
    path: string;
    scope: Scope;
    file: string;
    config?: PageConfig;
  }[];
  widgets: (Partial<WidgetConfig> & { file: string })[];
}

/**
 * Runs the built bundle with a stand-in dashboard to read what its pages and
 * widgets declare: nothing renders, only the modules' top level runs.
 */
function describe(code: string): Registered {
  const ui = new Proxy(
    { definePage: (c: unknown) => c, defineWidget: (c: unknown) => c },
    {
      get: (target, key) =>
        key in target ? target[key as keyof typeof target] : () => null,
    },
  );
  // The design system, as far as a module's top level can tell: anything,
  // called or read, is something more (a column helper, its columns).
  const anything: object = new Proxy(() => anything, {
    get: (_, key) => (key === Symbol.toPrimitive ? () => '' : anything),
    apply: () => anything,
  });
  let registered: Registered | undefined;
  const sandbox = {
    __rustrak: {
      React: {},
      jsxRuntime: {},
      ReactDOM: {},
      ReactQuery: {},
      ui,
      designSystem: anything,
    },
    __rustrakPlugins: {
      register: (_: string, def: Registered) => {
        registered = def;
      },
    },
  };
  runInNewContext(code, { ...sandbox, globalThis: sandbox });
  if (!registered) throw new Error('the built UI did not register itself');
  return registered;
}

/** One script and one stylesheet, the dashboard's modules left to it. */
async function bundle(
  dir: string,
  entry: string,
  outDir: string,
): Promise<void> {
  await build({
    root: dir,
    mode: 'production',
    configFile: false,
    logLevel: 'warn',
    plugins: [react(), tailwindcss()],
    build: {
      outDir,
      emptyOutDir: true,
      copyPublicDir: false,
      cssCodeSplit: false,
      lib: {
        entry,
        formats: ['iife'],
        name: '__rustrakPluginBundle',
        fileName: () => 'index.js',
        cssFileName: 'index',
      },
      rolldownOptions: {
        external: Object.keys(SHARED),
        output: { globals: SHARED },
      },
    },
  });
}
