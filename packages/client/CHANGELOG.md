# @rustrak/client

## 0.15.0

No changes in this release.

## 0.15.0-rc.3

No changes in this release.

## 0.15.0-rc.2

No changes in this release.

## 0.15.0-rc.1

No changes in this release.

## 0.15.0-rc.0

No changes in this release.

## 0.14.14

No changes in this release.

## 0.14.13

No changes in this release.

## 0.14.12

## 0.14.11

## 0.14.10

## 0.14.9

## 0.14.8

## 0.14.7

## 0.14.6

## 0.14.5

## 0.14.4

## 0.14.3

## 0.14.2

## 0.14.1

## 0.14.0

## 0.13.0

## 0.12.3

## 0.12.2

## 0.12.1

## 0.12.0

## 0.3.15

### Patch Changes

- [#194](https://github.com/rustrak/rustrak/pull/194) [`9a8b1bb`](https://github.com/rustrak/rustrak/commit/9a8b1bb34c815a6d2ffe23129f42a9cae2f5dc9b) Thanks [@AbianS](https://github.com/AbianS)! - ## Sentry Releases API

  Server implements `POST`/`PUT .../releases/...`, the endpoints `sentry-cli` and the Sentry JS bundler plugins (Next.js, SvelteKit, Nuxt, Remix) call on every build to create and finalize a release. Previously these 404'd, showing up in every build log for most self-hosted JS users. Adds a `releases` table (`project_id` + `version`, unique) backing the new endpoints.

  Regression clearing for issues resolved "in the next release" now compares real release creation dates instead of a string-inequality check, and runs automatically whenever a release is created — matching Sentry's own behavior of clearing pending resolutions on release creation.

  ## Removed: `POST /api/projects/{id}/deploys`

  This project-invented endpoint (and `@rustrak/client`'s `createDeploy` / `@rustrak/mcp`'s `record_deploy`) is removed. It existed only as a manual workaround to trigger the regression-clearing logic before release creation could do it automatically — creating a release now has the same effect, matching real Sentry (which has no such endpoint either; Sentry's own Deploy object is unrelated deploy-tracking metadata, not a regression-clearing trigger).

## 0.3.14

### Patch Changes

- [`d05105a`](https://github.com/rustrak/rustrak/commit/d05105aec39e7c44bcb459a43b3780377e221a2e) Thanks [@AbianS](https://github.com/AbianS)! - ## AI Agent Monitoring

  New Agents page tracks LLM-instrumented spans from any Sentry SDK: agent runs, duration, models by calls/tokens, tool calls, and a per-trace waterfall. Deliberately ships without a cost/spend estimate, since per-model pricing tables go stale too fast to promise, so Rustrak shows exact token counts instead.

  ## Sentry Spans Protocol v2

  Server now recognizes Spans Protocol v2, the batched wire format real Sentry SDKs (verified against @sentry/node + Vercel AI SDK) actually use for AI-instrumented spans. Previously only the legacy standalone-span format was parsed, so AI Agent Monitoring received no data from real SDKs. Also fixes cache/reasoning token attribute mapping and timestamp validation to match Relay's behavior.

  ## Standalone Span Ingestion

  Server accepts Sentry's standalone "span" envelope item (OTel-style spans without a parent transaction), the prerequisite for AI Agent Monitoring and general span-level querying via `GET /api/projects/{id}/spans`.

  ## Fixes & Docs

  - Source maps guide corrected for project/org resolution behavior and SvelteKit setup added
  - Docs build pinned to zod 4.3.5 to fix a CI-only shallow-clone failure with nextra

## 0.3.13

### Patch Changes

- [`50314dc`](https://github.com/rustrak/rustrak/commit/50314dc42960f5d5ddbd29cbc2d9111b7abfeae9) Thanks [@AbianS](https://github.com/AbianS)! - Added RUSTRAK_LOG_TIMEZONE environment variable for configuring server log timestamp display timezone. Updated dependencies across all packages. Fixed clippy compliance issue in notification service.

## 0.3.12

### Patch Changes

- [`b3a05e9`](https://github.com/rustrak/rustrak/commit/b3a05e979e47669a3ec665bfe0dae4e6bc2eeef3) Thanks [@AbianS](https://github.com/AbianS)! - ## Project Platform Auto-Detection

  Server automatically detects project platform from ingested events and exposes a `platform` field. The web UI renders platform-specific icons using platformicons. Client package now exposes `project.platform` in responses.

  ## Project Overview & Releases

  New project overview page with session trend charts and health score cards. New releases section with release environment cards and release list. Server adds releases and enhanced sessions API endpoints. Client adds releases and sessions resources.

  ## Sentry-Compatible UI Improvements

  Stack trace rendering now matches Sentry's behavior with in-app/system frame grouping, platform-adaptive formatting, and threads section. Breadcrumbs display with expand toggle, category icons, and color coding.

  ## Server Fixes

  Oversized events are now intelligently trimmed instead of being rejected outright. Source map rewriting also applies to thread frames, not just exception stacktraces.

## 0.3.11

### Patch Changes

- [`2686495`](https://github.com/rustrak/rustrak/commit/2686495ee671ef7ebdd319ed643e892c4f766bbf) Thanks [@AbianS](https://github.com/AbianS)! - - New Sentry-compatible issues model with status and priority lifecycle management, bulk operations (list stats, copy-as, packages context), and social features (share, bookmark, assign, snooze)
  - Issues web UI: new issue detail pages, event navigation with breadcrumbs, activity timeline, trend sparklines, collapsible sidebar
  - Token delete confirmation dialog in webview-ui settings
  - Agent-rusty now has access to the full getsentry/sentry monolith source for deeper Sentry compatibility analysis
  - Fixed is_resolved and is_muted shim logic to not interfere with muted/resolved issues
  - Fixed userReportSchema to accept empty-string email
  - Fixed 3 Sentry-compat divergences identified against the monolith source
  - Performance: list_stats now projects only `data->user` instead of full event blob
  - Dependencies updated to latest exact versions

## 0.3.10

### Patch Changes

- [`8406c44`](https://github.com/rustrak/rustrak/commit/8406c44154cbd730bd20a7563e013197b0651c8b) Thanks [@AbianS](https://github.com/AbianS)! - Storage cleanup now supports scoping to specific data types (events, transactions, logs, sessions). The server endpoint accepts optional data-type filter parameters, the MCP tools include `--events`, `--transactions`, `--logs`, and `--sessions` flags, the client forwards the filter options, and the WebView UI provides a data-type selection interface. Also fixes the cleanup success toast to correctly report when no issues were found.

## 0.3.9

### Patch Changes

- [`edad7dc`](https://github.com/rustrak/rustrak/commit/edad7dc0548ab184f708d878c4f8ae5963bbb9f5) Thanks [@AbianS](https://github.com/AbianS)! - Logs ingestion, storage, and retrieval pipeline with full SDK compatibility, including standalone log breadcrumb types. New webview-ui logs page with shadcn Table, sticky header, and dedicated sidebar entry. Client SDK logs resource and MCP list_logs tool added. Docs updated with logs usage guide.

## 0.3.8

### Patch Changes

- [`6286fd4`](https://github.com/rustrak/rustrak/commit/6286fd43b77bd4edd954fbd3254abf77c5dea15c) Thanks [@AbianS](https://github.com/AbianS)! - Added GET /api/tokens/{id} endpoint to reveal full token values. Updated client SDK tokens resource and MCP server tools accordingly. Fixed performance pages to use internal table scroll like the issues page, added password visibility toggle on login form, adapted storage settings layout for mobile, and updated GitHub links from personal to rustrak organization.

## 0.3.7

### Patch Changes

- [`8d4547e`](https://github.com/rustrak/rustrak/commit/8d4547e719c5fd683349e492f3065e792bca5145) Thanks [@AbianS](https://github.com/AbianS)! - Add storage usage tracking and data retention.

  The server now reports storage usage and supports configurable data retention, including manual storage cleanup and source-map garbage collection. A new storage settings page in webview-ui surfaces usage and cleanup controls. The TypeScript client and MCP package gain a storage resource/tool for programmatic access.

  Fixes:

  - SQLite: enable WAL mode and use BEGIN IMMEDIATE for digest writes to prevent dropped events under concurrent writes ([#131](https://github.com/rustrak/rustrak/issues/131), [#141](https://github.com/rustrak/rustrak/issues/141))
  - Support clipboard copying over HTTP, with improved fallback positioning (@WahidinAji, [#146](https://github.com/rustrak/rustrak/issues/146))
  - Correct local PostgreSQL development setup instructions (@WahidinAji, [#147](https://github.com/rustrak/rustrak/issues/147))

## 0.3.6

### Patch Changes

- [`d2642ba`](https://github.com/rustrak/rustrak/commit/d2642baaa51466e4fe79143113bc6c18fe241dba) Thanks [@AbianS](https://github.com/AbianS)! - Dedicated transaction and span processing pipeline added to the server with ingestion flow, migrations, models, and grouped performance UI in webview-ui featuring transaction detail, span waterfall chart, and stats table. Client and MCP packages updated with transaction API resources and tools. Documents performance protocol compatibility gaps vs the Sentry Relay pipeline.

## 0.3.5

### Patch Changes

- [`d0aa064`](https://github.com/rustrak/rustrak/commit/d0aa064b9d84d4ab86209e0d200cea51bf089ee3) Thanks [@AbianS](https://github.com/AbianS)! - Replace cursor-based pagination with offset-based pagination for the transactions API. Fix MCP package declaration output to ensure proper type exports (@jamilahmadzai). Update quinn-proto dependency and address various review feedback across the server, client, and UI packages.

## 0.3.4

### Patch Changes

- [`bd78a7e`](https://github.com/rustrak/rustrak/commit/bd78a7e8608ef6071480ab8563eef932320601de) Thanks [@AbianS](https://github.com/AbianS)! - Transaction ingestion pipeline with processor-pattern architecture, transaction detail endpoint, new performance dashboard UI with sidebar redesign, and client/MCP API wiring to support the new transaction endpoints

## 0.3.3

### Patch Changes

- [`a2b791b`](https://github.com/rustrak/rustrak/commit/a2b791b54e0db5630741c268dc1d14ec93b968cd) Thanks [@AbianS](https://github.com/AbianS)! - Release health period selector: the period parameter is now optional and configurable from the UI via a dropdown (24h, 48h, 7d). Previously the stats endpoint defaulted to 24h with no override. Also updates 35 JS and 11 Rust dependencies, removes 8 unused webview-ui packages, and fixes the Docker Rust base image version.

## 0.3.2

### Patch Changes

- [`8cf7a09`](https://github.com/rustrak/rustrak/commit/8cf7a09b2fa2006058dfad280cd215caf2aaa585) Thanks [@AbianS](https://github.com/AbianS)! - Session tracking and release health monitoring with full Sentry SDK compatibility, including session lifecycle management, crash-free rate aggregation, and a new release health dashboard. Added a dedicated changelog page to the documentation site. Various fixes for ingest handling of session-only envelopes, UI destructive button variants, Clippy warnings, and CI/release tooling.

## 0.3.1

### Patch Changes

- [#112](https://github.com/rustrak/rustrak/pull/112) [`174f439`](https://github.com/rustrak/rustrak/commit/174f4396749cac04fa2b07e0f90d3a76b67b0bd5) Thanks [@AbianS](https://github.com/AbianS)! - Add `/health/version` endpoint and display server version in About page. Expose version via client SDK and MCP tool.

## 0.3.0

### Minor Changes

- [`837ae98`](https://github.com/rustrak/rustrak/commit/837ae98c0d313aa20e54fc19a13f67f927e81e52) Thanks [@AbianS](https://github.com/AbianS)! - Add team management and project-level RBAC.

  **Server (`@rustrak/server`)**

  - New `teams`, `team_members`, `project_members` tables with migration
  - Team routes: create, get, update, delete, member management
  - Project member routes: add/remove members, role assignment (owner/admin/member)
  - `access` service: permission checks across all routes
  - RBAC extractors and middleware applied to projects, issues, events, source maps, alerts, tokens
  - `require_admin` middleware ordering fix on `list_channels`
  - Integration tests: `team_rbac_test.rs`

  **Client (`@rustrak/client`)**

  - New resources: `TeamResource`, `MembersResource`, `InvitationsResource`
  - New schemas and types: `team`, `member`, `invitation`
  - Updated `UserSchema` with role fields
  - Integration tests for all new resources

  **UI (`webview-ui`)**

  - Settings > Team page: invite members, list members, manage roles
  - Pending invitations list with accept/revoke
  - Project header with members dialog and role-based actions
  - `/invite/[token]` accept invitation flow
  - Hide global admins from project add-member list

  **MCP (`@rustrak/mcp`)**

  - New `team` tools: `list_team_members`, `invite_member`, `remove_member`, `update_member_role`
  - Fix alerts tools authorization (`require_admin` ordering)
  - Integration and unit tests for team tools

  **Docs**

  - New `usage/team.mdx`: team management guide
  - Updated `sdks/mcp.mdx`: team tools documentation

## 0.2.2

### Patch Changes

- [`f748f8c`](https://github.com/rustrak/rustrak/commit/f748f8cce27cb6599a2503aec74b257778b05866) Thanks [@AbianS](https://github.com/AbianS)! - feat(alerts): two-tier integrations with global credentials and per-rule routing override

  - Add alert integrations hub UI with collapsible section layout
  - Add two-tier alert routing: global channel credentials + per-rule override
  - Redesign alert rule form dialog
  - Remove legacy `channel_ids` field from alert rules
  - Add `alert-integrations` and `alert-channels` resources to client package
  - Fix source maps chunk upload to accept non-SHA1 multipart field names
  - Fix project event counts not decrementing when an issue is deleted
  - Regenerate OpenAPI spec with updated alert models

## 0.2.1

### Patch Changes

- [`5a0854b`](https://github.com/rustrak/rustrak/commit/5a0854bfd62e1e7e7267b89de248bfab40707b4c) Thanks [@AbianS](https://github.com/AbianS)! - chore: migrate repository to rustrak GitHub organization and Docker Hub

## 0.2.0

### Minor Changes

- [`fd768de`](https://github.com/rustrak/rustrak/commit/fd768de0816ba6eeeaa26ed8893d82bd6224fd2b) Thanks [@AbianS](https://github.com/AbianS)! - Add source map upload and stack frame rewriting support.

  ## @rustrak/server

  - **Source map processing pipeline** — New `POST /api/projects/{id}/files/` endpoint accepts artifact bundles (gzip/zip) and individual `.map` files via multipart upload, stores chunks, and assembles them asynchronously (workers/sourcemap_assembly.rs)
  - **Frame rewriting** — Digest worker now resolves minified stack frames to original source positions using stored source maps; file/line/col/context_line are rewritten in-place before event storage
  - **Assembly state machine** — chunk upload → assembly job → frame rewriting with retry logic; `retry_count` resets on re-queue; HTTP 200 with `missingChunks` field returned for assembly error state (Sentry protocol compliant)
  - **Migrations** — two new migrations: remove issue soft delete (`20260521`), source maps tables (`20260522`)
  - **Hard delete for issues** — `deleted_at` soft delete replaced with immediate CASCADE hard delete; reduces storage and simplifies queries

  ## @rustrak/client

  - **`SourceMapsResource`** — New resource class implementing the Sentry artifact bundle upload protocol: `createArtifactBundle()`, `uploadChunk()`, `assembleArtifacts()` with chunk-hash-keyed multipart fields
  - Exported from package root alongside existing resources

  ## webview-ui

  - Fix breadcrumb rendering — level badge and message display corrected after PR [#89](https://github.com/rustrak/rustrak/issues/89) review
  - Fix event display — improved titles, tags layout, and breadcrumb columns in event detail view

  ## docs

  - New `/usage/source-maps` page with upload guide and environment setup
  - Blog post: "Source Maps in Rust" covering the implementation approach
  - Updated environment reference with source map related config

## 0.1.3

### Patch Changes

- [`6a8b860`](https://github.com/rustrak/rustrak/commit/6a8b860eda0fa199a31089e295a102aef3da6122) Thanks [@AbianS](https://github.com/AbianS)! - Improve npm package metadata, READMEs, and official documentation.

  - Add `homepage`, `repository` (with monorepo `directory`), `bugs`, `author`, and `engines` fields to both packages
  - Expand `keywords` for better npm search discoverability
  - Add `README.md` to `@rustrak/client` published files (was missing)
  - Rewrite both package READMEs: badge row, prominent docs link, cross-references between packages, Cursor and Continue.dev config examples in `@rustrak/mcp`
  - Add new "SDKs & Integrations" section to the docs site with dedicated pages for `@rustrak/client` and `@rustrak/mcp` covering installation, full API reference, error handling, and AI client setup

## 0.1.2

### Patch Changes

- [`40ba761`](https://github.com/rustrak/rustrak/commit/40ba76136d2c455fc22fcc1b99850eb3d29769bd) Thanks [@AbianS](https://github.com/AbianS)! - **server**: migrate all alert-channel and alert-rule endpoints from `AuthenticatedUser` to `ApiAuth` extractor, enabling bearer token access to the alerts API

  **client**: remove `private` flag and add `publishConfig` for npm publishing; bump zod to 4.4.3, msw to 2.14.6, vitest to 4.1.6, @types/node to 25.8.0

  **mcp**: wire npm publish in CI pipeline for initial public release of `@rustrak/mcp`

## 0.1.1

### Patch Changes

- [#44](https://github.com/rustrak/rustrak/pull/44) [`4a84415`](https://github.com/rustrak/rustrak/commit/4a84415d867b5a1f15f11006278527671d62b242) Thanks [@AbianS](https://github.com/AbianS)! - Upgrade all dependencies to latest versions across the monorepo.

  - TypeScript 6.0.3 + Node.js engines >=22 across all packages
  - ky 2.x migration: `prefix` (was `prefixUrl`), updated hook signatures, removed 429 from retry list to avoid `Retry-After` sleep
  - lucide-react 1.x: replaced removed `Github` brand icon with inline SVG component
  - Rust: actix-web 4.13, actix-session 0.11, tokio 1.52, sqlx 0.8.6, rand 0.10 (`RngExt`), sha2 0.11 (`hex::encode`), hmac 0.13 (`KeyInit`)
