<div align="center">
  <a href="https://rustrak.dev">
    <img src="https://raw.githubusercontent.com/rustrak/rustrak/main/apps/docs/public/logo.svg" alt="Rustrak" width="64" height="64" />
  </a>
  <h1>@rustrak/plugin</h1>
  <p>The framework and compiler for <a href="https://rustrak.dev">Rustrak</a> plugins</p>

  <p>
    <a href="https://www.npmjs.com/package/@rustrak/plugin">
      <img src="https://img.shields.io/npm/v/@rustrak/plugin?style=flat-square&color=cb3837" alt="npm version" />
    </a>
    <a href="https://www.npmjs.com/package/@rustrak/plugin">
      <img src="https://img.shields.io/npm/dw/@rustrak/plugin?style=flat-square" alt="weekly downloads" />
    </a>
    <a href="https://github.com/rustrak/rustrak/blob/main/LICENSE">
      <img src="https://img.shields.io/npm/l/@rustrak/plugin?style=flat-square" alt="license" />
    </a>
    <a href="https://github.com/rustrak/rustrak/actions/workflows/ci.yml">
      <img src="https://img.shields.io/github/actions/workflow/status/rustrak/rustrak/ci.yml?style=flat-square&label=CI" alt="CI" />
    </a>
  </p>

  <p>
    <a href="https://github.com/rustrak/plugins">Official plugins</a>
    ·
    <a href="https://github.com/rustrak/rustrak">GitHub</a>
    ·
    <a href="https://github.com/rustrak/rustrak/issues">Report a Bug</a>
  </p>
</div>

---

`@rustrak/plugin` is how a [Rustrak](https://rustrak.dev) plugin is written and built. A plugin is a Rust server compiled to a sandboxed WebAssembly component, with its own SQLite database, settings and permissions, plus React pages that the dashboard draws as its own. This package gives you three things:

- **`rustrak-plugin`**, the CLI: creates a project, generates its types and packs it into a `.rtp` the server installs
- **`defineConfig`**, the typed `rustrak.config.ts`: name, supported Rustrak versions, settings and permissions
- **`@rustrak/plugin/ui`**, what a plugin's pages import: data hooks, settings, permissions, links, filters and the dashboard's own controls

The Rust side is the [`rustrak-plugin`](https://crates.io/crates/rustrak-plugin) crate. Both are released with the server and share its version number.

> Plugins are in beta. The API can change between minor versions while Rustrak is on `0.x`.

## Requirements

- Node.js ≥ 22.18 (`rustrak.config.ts` loads through Node's own type stripping)
- Rust with the `wasm32-wasip2` target: `rustup target add wasm32-wasip2`
- A Rustrak server ≥ 0.34.0 to install the plugin on

## Quick Start

```bash
npx @rustrak/plugin new visits
cd visits
npm install
npm run build        # dist/visits-0.1.0.rtp
```

The new project builds as it is: a route that records page views, one that counts them, a page in each project, a widget under every issue, a setting, a permission and texts in English and Spanish. Upload `dist/visits-0.1.0.rtp` in **Settings › Plugins** and it runs.

## Project layout

```
visits/
├── rustrak.config.ts        name, Rustrak versions, settings, permissions
├── package.json             id and version of the plugin
├── Cargo.toml               the Rust crate (crate root is generated)
├── src/
│   ├── server/              Rust: routes/ and models/
│   ├── ui/
│   │   ├── routes/project/  pages inside a project
│   │   ├── routes/instance/ pages outside any project
│   │   └── widgets/         drawn into the dashboard's own pages
│   ├── messages/            en.json, es.json…
│   └── public/              files served to anyone (a tracker script)
├── migrations/              SQL, run in order on the plugin's database
├── readme/en.md             its page in the catalog (required)
└── logo.svg                 its mark (optional)
```

`.rustrak/` is written by the CLI from the rest and is never committed.

## Configuration

```typescript
import { defineConfig } from '@rustrak/plugin';

export default defineConfig({
  name: 'Visits',
  rustrak: '>=0.34.0',
  icon: 'performance',
  storageMb: 512,
  permissions: {
    visits: { scope: 'project', actions: ['read'] },
  },
  settings: {
    keepQuery: { type: 'boolean', scope: 'project', default: false },
    apiKey: { type: 'secret', required: true },
  },
});
```

| Field | What |
|---|---|
| `name` | Shown in the catalog and the dashboard |
| `rustrak` | The Rustrak versions it runs on: `>=0.34.0`, `^0.34.0`, `>=0.34.0, <0.40.0` |
| `id` | Optional. Defaults to the package name, without scope or `rustrak-plugin-` prefix |
| `icon` | A `@rustrak/ui` icon name, used when there is no `logo.svg` |
| `storageMb` | Quota of the plugin's database, in MB. Defaults to 512 |
| `settings` | What the admin can change. Rustrak draws the form |
| `permissions` | What people can be granted, beside Rustrak's own, as `<plugin>:<resource>` |

Settings are `text`, `url`, `secret`, `number`, `boolean` or `select`, set once for the instance or per project. A `secret` is never sent to the browser. A `required` setting makes the plugin answer 503 until it is set.

Every choice is a union of what Rustrak accepts, so a mistake is an error in the editor, not at install. Rustrak checks it again when it installs.

Labels come from `src/messages/`: `settings.<key>.label` and `permissions.<resource>.label`, each with an optional `.description`.

## Server

Routes are Rust functions in `src/server/`:

```rust
use rustrak_plugin::prelude::*;
use crate::Perm;

/// Public: an app sends it with its project's DSN key.
#[post("/visit", public)]
fn record(ctx: &Ctx, body: Body) -> Result<NoContent> {
    ctx.db().execute(
        "INSERT INTO visits (project_id, path, created_at) VALUES (?1, ?2, unixepoch())",
        params![ctx.project_id(), body.text()],
    )?;
    Ok(NoContent)
}

/// For signed-in people who hold visits:visits:read.
#[get("/visits", requires = Perm::Visits.read())]
fn count(ctx: &Ctx) -> Result<String> {
    let n: i64 = ctx.db().scalar(
        "SELECT count(*) FROM visits WHERE project_id = ?1",
        params![ctx.project_id()],
    )?;
    Ok(n.to_string())
}
```

`Settings` and `Perm` are generated from `rustrak.config.ts`: `Settings::get(ctx)?.keep_query` is typed. A table with a `created_at` column (unix seconds) is history, and its old rows are cleared with the retention the admin sets in Storage.

## Pages and widgets

A page is a `page.tsx` under `src/ui/routes/project` or `src/ui/routes/instance`. Folders are its path, and `[name]` is a parameter.

```tsx
import { definePage, usePluginApi, usePluginTranslations } from '@rustrak/plugin/ui';
import { Figure, FigureStrip, SettingsHeader } from '@rustrak/ui';

export const config = definePage({
  sidebar: { label: 'nav.visits', icon: 'performance', rank: 1 },
  requires: 'visits:read',
});

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
```

A widget is a file in `src/ui/widgets/` that draws into one of the dashboard's own pages:

| Zone | Where | Props |
|---|---|---|
| `issue.details.after` | Under an issue's charts | `projectId`, `issue` |
| `project.overview.after` | At the end of a project's overview | `projectId` |
| `plugin.setup` | In the plugin's settings, under each project it runs in | `projectId` |

```tsx
export const config = defineWidget({ zone: 'issue.details.after' });
```

A plugin never bundles React, TanStack Query, `use-intl` or `@rustrak/ui`: the dashboard hands it its own. So a plugin page uses the components the dashboard is built from and looks like the rest of Rustrak.

### What `@rustrak/plugin/ui` exports

| Export | What |
|---|---|
| `definePage`, `defineWidget` | A page's or widget's `config` |
| `usePluginApi`, `usePluginMutation`, `usePluginFetch` | The plugin's own routes, through the dashboard's query cache |
| `useRustrakApi` | Rustrak's API, as the signed-in reader |
| `usePluginSettings` | The plugin's settings, without its secrets |
| `usePluginPermissions` | `can()` over the plugin's permissions |
| `usePluginTranslations`, `useFormatter`, `useLocale` | Texts from `src/messages/`, dates and numbers in the reader's language |
| `usePluginSearch`, `PluginViews`, `PeriodFilter` | Filters in the address, saved views and the period picker |
| `usePluginTrail`, `usePluginNavigate`, `PluginLink`, `HostLink` | Breadcrumbs and links inside the plugin and to Rustrak's pages |
| `usePlugin`, `publicUrl`, `setupHref` | The plugin's context, its public files and its setup page |
| `PluginApiError` | What a failed call throws, with its status |

Every type is checked against the project: `usePluginTranslations()` knows the keys in `src/messages/en.json`, and `requires` knows the permissions in `rustrak.config.ts`.

## CLI

| Command | What |
|---|---|
| `rustrak-plugin new <name>` | Creates a plugin project that builds as it is |
| `rustrak-plugin generate` | Writes `.rustrak/` from `rustrak.config.ts` and `src/messages/`: types for the editor, `Settings` and `Perm` for Rust |
| `rustrak-plugin build` | Compiles and packs the plugin into `dist/<id>-<version>.rtp` |
| `rustrak-plugin index <dir>` | Writes a registry's `index.json` from the `.rtp` files in `<dir>` |

`build` does, in order:

1. Refuses a project without `readme/en.md`, its page in the catalog
2. Runs the migrations on an in-memory SQLite, so one that fails does it here, not at install
3. Compiles `src/server/` to a `wasm32-wasip2` component with a fixed release profile (`opt-level = "s"`, LTO, stripped, `panic = "abort"`), whatever `Cargo.toml` says
4. Builds the pages and widgets against the dashboard's React and design system
5. Packs the manifest, the wasm, the UI, migrations, public files, texts, readmes and logo into a `.rtp`, with the sha256 of every file

A text missing in a language, or a readme missing for one, is a warning, not an error: English is the fallback.

### Publishing a registry

A registry is a folder of `.rtp` files and a signed `index.json`:

```bash
rustrak-plugin index ./packages
minisign -Sm ./packages/index.json
```

Point a server at it with `RUSTRAK_PLUGIN_REGISTRY`, and trust its key with `RUSTRAK_PLUGIN_REGISTRY_KEYS`. The official registry is [rustrak/plugins](https://github.com/rustrak/plugins).

## Related Packages

| Package | Description |
|---|---|
| [`rustrak-plugin`](https://crates.io/crates/rustrak-plugin) | The Rust framework: routes, typed context, the plugin's database, settings and permissions |
| [`@rustrak/ui`](https://www.npmjs.com/package/@rustrak/ui) | The design system the dashboard and every plugin page are built from |
| [`@rustrak/client`](https://www.npmjs.com/package/@rustrak/client) | The TypeScript client for Rustrak's API |
| [rustrak/plugins](https://github.com/rustrak/plugins) | The official plugins and their signed registry |

## What is Rustrak?

[Rustrak](https://rustrak.dev) is a self-hosted error tracking server written in Rust that accepts events from any Sentry SDK: point the SDK's DSN at it and nothing else changes. One container serves the API and the dashboard.

- [Getting Started](https://rustrak.dev/getting-started/overview)
- [Self-Hosting Guide](https://rustrak.dev/configuration/production)
- [API Reference](https://rustrak.dev/api-reference)
- [GitHub](https://github.com/rustrak/rustrak)

## License

[MIT](https://github.com/rustrak/rustrak/blob/main/LICENSE)
