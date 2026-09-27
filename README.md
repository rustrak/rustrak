<div align="center">

<img width="2000" height="1080" alt="hero" src="https://github.com/user-attachments/assets/6be3f33e-c17a-4aa9-a082-656639c3ad29" />

[![CI](https://github.com/rustrak/rustrak/actions/workflows/ci.yml/badge.svg)](https://github.com/rustrak/rustrak/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/rustrak/rustrak)](https://github.com/rustrak/rustrak/releases)
[![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)
[![Docker pulls](https://img.shields.io/docker/pulls/rustrak/rustrak-server)](https://hub.docker.com/r/rustrak/rustrak-server)
[![Sponsors](https://img.shields.io/github/sponsors/AbianS?label=sponsors&color=db61a2)](https://github.com/sponsors/AbianS)

**[Documentation](https://rustrak.github.io/rustrak/)** ·
[Quickstart](#quickstart) ·
[Changelog](https://rustrak.github.io/rustrak/changelog) ·
[Report a bug](https://github.com/rustrak/rustrak/issues)

</div>

Rustrak is a self-hosted error tracker that also takes logs, transactions,
release health and AI-agent traces.

It speaks the Sentry protocol rather than a lookalike of it, so the SDK you
already have points at Rustrak by changing one string. No new agent to install,
no vendor library to swap, no code to rewrite. It runs as a single process
with SQLite by default, which means the whole install is one compose file and
nothing to provision.

<img width="2000" height="1000" alt="wp-overview" src="https://github.com/user-attachments/assets/807fa637-a111-4e4c-8d16-de9d86b448ee" />

## Why Rustrak

Error tracking usually comes two ways: a SaaS bill that scales with your worst
day, or a self-hosted stack that wants its own machine. Rustrak is the third
option: the same protocol, on hardware you already have.

Two design decisions do most of that work. **The dashboard is static files the
server hands out**, so one small process answers both the API and the UI, and
an image built without the dashboard is a complete product on its own. And
**ingestion is two-phase**. The endpoint parses the envelope, writes
it to disk and returns `200`; a spawned task then does the database work.
Accepting an event never waits on the database, which is what stops a traffic
spike from becoming a timeout inside your app. On a 4-core box with SQLite, a
burst of 20,000 events is accepted at about 5k events per second with the
server peaking under 60 MB of memory.

<img width="2000" height="820" alt="numbers" src="https://github.com/user-attachments/assets/e51ca5d2-56ed-4366-966d-24801b47f0f8" />

No per-event pricing, no sampling you did not ask for, and no seat you have to
justify to anyone.

## Quickstart

SQLite is the default, so this is the whole install. There is no database to
provision and no broker to run.

This is the repository's `docker-compose.yml`:

```yaml
services:
  server:
    image: rustrak/rustrak-server:latest
    ports: ["8080:8080"]
    volumes: [rustrak_data:/data]
    environment:
      - SESSION_SECRET_KEY=${SESSION_SECRET_KEY}
      - CREATE_SUPERUSER=${CREATE_SUPERUSER}
    restart: unless-stopped

volumes:
  rustrak_data:
```

```bash
export SESSION_SECRET_KEY=$(openssl rand -hex 32)
export CREATE_SUPERUSER=admin@example.com:changeme123
docker compose up -d
```

Open <http://localhost:8080> and sign in with those credentials. One container
answers both the dashboard and the API, so there is no second port and no
address to tell one half about the other. Want the dashboard on a different
host from your data? `rustrak/rustrak-ui` is the same dashboard behind nginx;
see the [production guide](https://rustrak.github.io/rustrak/configuration/production#dashboard-on-its-own-host).

Running at scale? `docker-compose.postgres.yml` runs the `:postgres` image next
to a PostgreSQL service. The
[installation guide](https://rustrak.github.io/rustrak/getting-started/installation)
has both setups and the production notes.

## Point your SDK at it

Rustrak accepts the standard Sentry envelope, so migrating is a configuration
change rather than a project. Create a project, copy its DSN, and change one
line. Your SDK never learns it is talking to something else.

<img width="2000" height="700" alt="dsn" src="https://github.com/user-attachments/assets/0efe0ef7-c4e4-481e-99ab-d9dabd56f135" />

```python
# Python
import sentry_sdk
sentry_sdk.init(dsn="http://<key>@localhost:8080/<project_id>")
```

```javascript
// JavaScript
import * as Sentry from "@sentry/browser";
Sentry.init({ dsn: "http://<key>@localhost:8080/<project_id>" });
```

```go
// Go
sentry.Init(sentry.ClientOptions{Dsn: "http://<key>@localhost:8080/<project_id>"})
```

Any Sentry SDK works, in any language. If you are already sending to Sentry, you
can point a second DSN at Rustrak and compare the two before committing.

## What's inside

### Errors

Events are grouped into issues by a deterministic fingerprint: the one your SDK
sent if it sent one, otherwise exception type plus the first line of the message
plus the transaction. Same input, same group, every time, so an issue does not
split in two after a deploy.

From the list you can triage in bulk, filter by status, and read each issue's
24-hour trend without opening anything. Upload source maps and a minified frame
resolves back to the line you actually wrote.

<img width="2000" height="800" alt="wp-issues" src="https://github.com/user-attachments/assets/0402007b-25fe-48a9-adb7-83317f8cf1ca" />

### AI agent traces

Turn on your SDK's AI integration and agent runs appear on their own page: runs
over time, duration percentiles, top models by calls and by tokens, top tools,
and a per-trace waterfall of every LLM call, tool call and handoff in order.

Rustrak reads Sentry's Spans Protocol v2, the batched format the real SDKs use
for AI spans, verified against `@sentry/node` with the Vercel AI SDK. It also
takes standalone OTel-style spans that have no parent transaction. When one agent
hands off to another mid-run, the trace lists every agent involved, not just the
first.

<img width="2000" height="920" alt="wp-agents" src="https://github.com/user-attachments/assets/8867b954-e2bf-41bf-a5b9-2f484e640bba" />

There is no server-side setup. One integration in `Sentry.init` is the whole
change:

```javascript
import * as Sentry from '@sentry/node';

Sentry.init({
  dsn: 'http://<key>@localhost:8080/<project_id>',
  integrations: [Sentry.vercelAIIntegration()],
});
```

And there is deliberately **no spend widget**. Per-model pricing goes stale
faster than we could ship it, so Rustrak reports exact token counts and leaves
the multiplication to you.

### Performance

Transactions and spans flow through a processor pipeline onto a performance
dashboard, ranked by p95. Latency is coloured against thresholds rather than left
as raw numbers: green under a second, amber under three, red beyond. The row that
needs attention is the one that looks wrong.

Open a transaction for its span waterfall and its measurements.

<img width="2000" height="840" alt="wp-performance" src="https://github.com/user-attachments/assets/0f05bbdc-0cce-49f0-9926-c8760a60b21b" />

### Logs

Errors tell you what broke. Logs tell you what your app was doing when it did
not. Rustrak stores structured logs as a first-class event type on the same
envelope endpoint, with six severities from `trace` to `fatal`, their own filter,
and per-row attributes that keep their types.

A log emitted inside an active span carries the `trace_id`, so it links back to
the request it came from.

<img width="2000" height="760" alt="wp-logs" src="https://github.com/user-attachments/assets/bef1f739-1b45-4dea-8350-72efd89f1a4a" />

Logging is off by default in Sentry SDKs. Opt in once, then use the logger:

```javascript
Sentry.init({
  dsn: 'http://<key>@localhost:8080/<project_id>',
  enableLogs: true,
});

Sentry.logger.info('User signed up', { userId: 4172, plan: 'pro' });
```

### Release health

Sessions are tracked with full Sentry SDK compatibility and aggregated per
release, which turns "is this deploy worse than the last one" into two numbers:
**crash-free sessions** and **crash-free users**.

Both are tiered rather than printed flat: green at 99% or above, amber at 95%,
red below. A release at 96% is not fine and should not read as if it were.

<img width="2000" height="800" alt="wp-releases" src="https://github.com/user-attachments/assets/db9906a5-e8b5-4ac9-83ce-99f4e61437c7" />

### Alerts

Alert rules fire on three events, and each rule chooses its own destination:

| Trigger | When it fires |
|---|---|
| `new_issue` | A new issue is detected for the first time |
| `regression` | A resolved issue reappears |
| `unmute` | A muted issue is unmuted |

Destinations are Slack (incoming webhook or bot token), SMTP email, or a plain
JSON webhook. Credentials belong to the instance and are configured once;
routing belongs to the rule.

<img width="2000" height="660" alt="wp-alerts" src="https://github.com/user-attachments/assets/6e677c44-4876-4c9c-ac91-be4e30e0b7f6" />

### Teams, storage and retention

Projects carry three roles, **Admin**, **Member** and **Viewer**, with
invitations, so a contractor can be given one project and nothing else.

The server also reports its own storage usage and enforces configurable
retention, with manual cleanup and source-map garbage collection on demand. That
matters more on a self-hosted instance than on a hosted one: nobody else is going
to notice the disk filling up.

### Getting around

`⌘K` opens a command bar over any screen. Projects and settings are two fixed
sections, with the highlighted project's pages in a preview column, so Enter on
a project goes to the project and its pages stay one keystroke away. Typing
flattens everything into one relevance-ordered list.

## From code, and from your editor

The REST API has a typed client, and the client has an MCP server over it, so an
assistant can triage your instance in the same session it is writing the fix.

```bash
npm install @rustrak/client   # the REST API, typed
npx @rustrak/mcp              # Claude, Cursor, Continue
```

| Package | Version | What it is |
|---|---|---|
| [`@rustrak/client`](https://www.npmjs.com/package/@rustrak/client) | [![npm](https://img.shields.io/npm/v/@rustrak/client?style=flat-square)](https://www.npmjs.com/package/@rustrak/client) | TypeScript client. Every method returns `Result<T, RustrakError>` and never throws, so a call that can fail says so in its type |
| [`@rustrak/mcp`](https://www.npmjs.com/package/@rustrak/mcp) | [![npm](https://img.shields.io/npm/v/@rustrak/mcp?style=flat-square)](https://www.npmjs.com/package/@rustrak/mcp) | MCP server over that client, so an assistant can read and manage your instance |

The OpenAPI spec is browsable in the [API reference](https://rustrak.github.io/rustrak/api-reference).

## Telemetry

The server sends one anonymous report every six hours: version, platform,
memory, and counters such as "how many ingests were rejected" or "which
routes returned 500". Never IPs, names, URLs, events or messages. It is what
lets a memory regression or a new panic show up across installations after
a release, and it costs one atomic increment per request.

```bash
RUSTRAK_TELEMETRY=off   # or DO_NOT_TRACK=1; either one is enough
```

Every field is listed in the
[telemetry page](https://rustrak.github.io/rustrak/configuration/telemetry),
and `GET /api/telemetry/preview` shows the exact document before it leaves.

## Documentation

| | |
|---|---|
| [Getting started](https://rustrak.github.io/rustrak/getting-started/overview) | What Rustrak is and how it fits together |
| [Installation](https://rustrak.github.io/rustrak/getting-started/installation) | Self-hosting, PostgreSQL, production notes |
| [Configuration](https://rustrak.github.io/rustrak/configuration/environment) | Every environment variable |
| [API reference](https://rustrak.github.io/rustrak/reference/api) | Endpoints and schemas |
| [Architecture](https://rustrak.github.io/rustrak/reference/architecture) | Two-phase ingestion, grouping, storage |

## Contributors

People who have contributed code, translations or documentation to Rustrak.

<div align="center">

<a href="https://github.com/rustrak/rustrak/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=rustrak/rustrak" alt="Rustrak contributors" />
</a>

</div>

[CONTRIBUTING.md](CONTRIBUTING.md) covers the local setup, the test suite and
the quality gate. Issues tagged
[good first issue](https://github.com/rustrak/rustrak/labels/good%20first%20issue)
are a reasonable place to start.

## Sponsors

Rustrak has no paid tier and no hosted plan. Development is funded through
GitHub Sponsors.

<table align="center">
<tr>
<td align="center" width="180">
  <a href="https://github.com/scorewarrior">
    <img src="https://avatars.githubusercontent.com/u/135209230?s=180&v=4" width="90" height="90" alt="Scorewarrior" />
    <br />
    <sub><b>Scorewarrior</b></sub>
  </a>
</td>
</tr>
</table>

<div align="center">

<a href="https://github.com/sponsors/AbianS">
  <img src="https://img.shields.io/badge/Sponsor%20Rustrak-EA4AAA?style=for-the-badge&logo=githubsponsors&logoColor=white" alt="Sponsor Rustrak on GitHub" />
</a>

</div>

## License

GPL-3.0. See [LICENSE](LICENSE). Copyright © 2026 Abian Suarez.
