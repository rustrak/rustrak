// `rustrak-plugin new <dir>`: a plugin project that builds as it is.

import {
  existsSync,
  mkdirSync,
  readdirSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { createRequire } from 'node:module';
import { basename, dirname, join } from 'node:path';
import type { PluginConfig } from './config.ts';
import { generate } from './generate.ts';

/** The first Rustrak release with this world. */
const MIN_RUSTRAK = '>=0.34.0';
/** `@rustrak/plugin`, `@rustrak/ui` and the `rustrak-plugin` crate share the Rustrak release's number. */
export const RELEASE: string = createRequire(import.meta.url)(
  '../package.json',
).version;
const RUSTRAK_PACKAGES = `^${RELEASE}`;

/** An object as TypeScript source: unquoted keys, single quotes. */
const asTs = (value: unknown) =>
  JSON.stringify(value, null, 2)
    .replace(/"([A-Za-z_][A-Za-z0-9_]*)":/g, '$1:')
    .replaceAll('"', "'")
    // A list of words on one line, and a trailing comma after every entry.
    .replace(
      /\[\s+((?:'[^']*',\s+)*'[^']*')\s+\]/g,
      (_, items: string) => `[${items.replace(/,\s+/g, ', ')}]`,
    )
    // Twice: a closing brace that took a comma can end the next entry too.
    .replace(/([^,{[\s])(\n\s*[}\]])/g, '$1,$2')
    .replace(/([^,{[\s])(\n\s*[}\]])/g, '$1,$2');

const title = (slug: string) =>
  slug
    .split('-')
    .filter(Boolean)
    .map((w) => w[0]!.toUpperCase() + w.slice(1))
    .join(' ');

/** Why `dir` cannot take a new plugin; `undefined` when it can. */
export function nameError(dir: string): string | undefined {
  const slug = basename(dir);
  if (!/^[a-z][a-z0-9-]{1,31}$/.test(slug)) {
    return `${slug}: a plugin name is 2 to 32 lowercase letters, digits or dashes`;
  }
  if (!existsSync(dir)) return undefined;
  if (!statSync(dir).isDirectory()) return `${dir} is a file, not a directory`;
  if (readdirSync(dir).length > 0) return `${dir} is not empty`;
}

/** Writes the project; how many files it wrote. */
export function scaffold(dir: string): number {
  const error = nameError(dir);
  if (error) throw new Error(error);
  const slug = basename(dir);
  mkdirSync(dir, { recursive: true });

  // Written to rustrak.config.ts and generated from at once: the new project
  // has no node_modules yet to load it back with.
  const config: PluginConfig = {
    name: title(slug),
    rustrak: MIN_RUSTRAK,
    icon: 'performance',
    permissions: { visits: { scope: 'project', actions: ['read'] } },
    settings: {
      keepQuery: { type: 'boolean', scope: 'project', default: false },
    },
  };

  const files: Record<string, string> = {
    'package.json': `${JSON.stringify(
      {
        name: slug,
        version: '0.1.0',
        description: `${title(slug)}, a Rustrak plugin.`,
        license: 'MIT',
        type: 'module',
        scripts: { build: 'rustrak-plugin build' },
        devDependencies: {
          '@rustrak/plugin': RUSTRAK_PACKAGES,
          '@rustrak/ui': RUSTRAK_PACKAGES,
          '@tanstack/react-query': '^5.100.0',
          '@types/react': '^19.0.0',
          react: '^19.0.0',
          typescript: '^6.0.0',
          'use-intl': '^4.0.0',
        },
      },
      null,
      2,
    )}\n`,

    'rustrak.config.ts': `import { defineConfig } from '@rustrak/plugin';

export default defineConfig(${asTs(config)});
`,

    'Cargo.toml': `# The Rust side of the plugin: everything under src/server/. Its crate root,
# .rustrak/lib.rs, is written by rustrak-plugin from those folders.
[package]
name = "${slug}"
version = "0.1.0"
edition = "2024"
publish = false

[lib]
path = ".rustrak/lib.rs"
crate-type = ["cdylib"]

[dependencies]
rustrak-plugin = "${RELEASE}"
serde = { version = "1", features = ["derive"] }

# A plugin builds on its own, never as part of a parent workspace.
[workspace]
`,

    'src/server/models/visit.rs': `//! A page view, kept in the plugin's own database. Its \`created_at\` makes
//! it history: how long it is kept is the admin's choice, in Rustrak's
//! Storage page, not the plugin's.

use rustrak_plugin::prelude::*;

/// Records a visit, its query string left out unless the project keeps it.
pub fn record(ctx: &Ctx, path: &str) -> Result<()> {
    let path = if crate::Settings::get(ctx)?.keep_query {
        path
    } else {
        path.split('?').next().unwrap_or(path)
    };
    ctx.db().execute(
        "INSERT INTO visits (project_id, path, created_at) VALUES (?1, ?2, unixepoch())",
        params![ctx.project_id(), path],
    )?;
    Ok(())
}

pub fn count(ctx: &Ctx) -> Result<i64> {
    Ok(ctx.db().scalar(
        "SELECT count(*) FROM visits WHERE project_id = ?1",
        params![ctx.project_id()],
    )?)
}

`,

    'src/server/routes/visits.rs': `use rustrak_plugin::prelude::*;

use crate::models::visit;
use crate::Perm;

/// Public: an app sends it with its project's DSN key.
#[post("/visit", public)]
fn record(ctx: &Ctx, body: Body) -> Result<NoContent> {
    visit::record(ctx, &body.text())?;
    Ok(NoContent)
}

/// For signed-in people who hold ${slug}:visits:read.
#[get("/visits", requires = Perm::Visits.read())]
fn count(ctx: &Ctx) -> Result<String> {
    Ok(visit::count(ctx)?.to_string())
}
`,

    'src/ui/routes/project/page.tsx': `import {
  definePage,
  usePluginApi,
  usePluginTranslations,
} from '@rustrak/plugin/ui';
import { Figure, FigureStrip, SettingsHeader } from '@rustrak/ui';

export const config = definePage({
  sidebar: { label: 'nav.visits', icon: 'performance', rank: 1 },
});

/** The plugin's page in each project. */
export default function Visits() {
  const t = usePluginTranslations();
  const visits = usePluginApi<string>('/visits');
  return (
    <div className="flex flex-col gap-4 p-page-gutter">
      <SettingsHeader title={t('visits.title')} description={t('visits.subtitle')} />
      <FigureStrip className="grid-cols-1">
        <Figure label={t('visits.total')} value={visits.data ?? '…'} />
      </FigureStrip>
    </div>
  );
}
`,

    'src/ui/widgets/issue-visits.tsx': `import {
  defineWidget,
  usePluginApi,
  usePluginTranslations,
  type WidgetProps,
} from '@rustrak/plugin/ui';
import { Figure, FigureStrip } from '@rustrak/ui';

export const config = defineWidget({ zone: 'issue.details.after' });

/** Drawn under every issue of a project where the plugin is active. */
export default function IssueVisits(_: WidgetProps<'issue.details.after'>) {
  const t = usePluginTranslations();
  const visits = usePluginApi<string>('/visits');
  return (
    <FigureStrip className="grid-cols-1">
      <Figure label={t('visits.total')} value={visits.data ?? '…'} />
    </FigureStrip>
  );
}
`,

    'src/messages/en.json': `${JSON.stringify(
      {
        nav: { visits: title(slug) },
        visits: {
          title: title(slug),
          subtitle: 'Every page view this project sent.',
          total: 'Visits',
        },
        permissions: {
          visits: {
            label: 'Visits',
            description: 'See how many visits a project has.',
          },
        },
        settings: {
          keepQuery: {
            label: 'Keep query strings',
            description: 'Store ?utm_source=… with each path.',
          },
        },
      },
      null,
      2,
    )}\n`,

    'src/messages/es.json': `${JSON.stringify(
      {
        nav: { visits: title(slug) },
        visits: {
          title: title(slug),
          subtitle: 'Cada vista de página que envió este proyecto.',
          total: 'Visitas',
        },
        permissions: {
          visits: {
            label: 'Visitas',
            description: 'Ver cuántas visitas tiene un proyecto.',
          },
        },
        settings: {
          keepQuery: {
            label: 'Guardar la query',
            description: 'Guardar ?utm_source=… con cada ruta.',
          },
        },
      },
      null,
      2,
    )}\n`,

    'tsconfig.json': `${JSON.stringify(
      {
        compilerOptions: {
          target: 'ES2022',
          module: 'ESNext',
          moduleResolution: 'bundler',
          jsx: 'react-jsx',
          strict: true,
          noEmit: true,
          skipLibCheck: true,
        },
        include: ['src/ui', 'rustrak.config.ts', '.rustrak/plugin-env.d.ts'],
      },
      null,
      2,
    )}\n`,

    'migrations/0001_init.sql': `-- created_at (unix seconds) makes a table history: Rustrak's Storage
-- page clears its old rows with the retention the admin sets.
CREATE TABLE visits (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL,
    path TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
`,

    // Its page in the catalog, one per language; English is the fallback.
    'readme/en.md': `# ${title(slug)}

What this plugin does, for the Rustrak plugin catalog.

\`\`\`bash
npm install
npm run build   # dist/${slug}-0.1.0.rtp
\`\`\`
`,
    'readme/es.md': `# ${title(slug)}

Qué hace este plugin, para el catálogo de plugins de Rustrak.

\`\`\`bash
npm install
npm run build   # dist/${slug}-0.1.0.rtp
\`\`\`
`,

    // Its mark wherever Rustrak shows it: replace with the plugin's own.
    'logo.svg': `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="8" fill="#a3e635"/><text x="16" y="21.5" text-anchor="middle" font-family="sans-serif" font-size="15" font-weight="700" fill="#1a1a1a">${title(slug)[0]}</text></svg>\n`,

    '.gitignore': '/target\n/dist\n/node_modules\n/.rustrak\n',
  };

  for (const [path, text] of Object.entries(files)) {
    const file = join(dir, path);
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, text);
  }
  // .rustrak/ now, so cargo and the editor work before the first build.
  generate(dir, config);
  return Object.keys(files).length;
}
