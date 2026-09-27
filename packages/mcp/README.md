<div align="center">
  <a href="https://rustrak.github.io/rustrak">
    <img src="https://raw.githubusercontent.com/rustrak/rustrak/main/apps/docs/public/logo.svg" alt="Rustrak" width="64" height="64" />
  </a>
  <h1>@rustrak/mcp</h1>
  <p>MCP server that gives AI assistants full control of your <a href="https://rustrak.github.io/rustrak">Rustrak</a> error tracking</p>

  <p>
    <a href="https://www.npmjs.com/package/@rustrak/mcp">
      <img src="https://img.shields.io/npm/v/@rustrak/mcp?style=flat-square&color=cb3837" alt="npm version" />
    </a>
    <a href="https://www.npmjs.com/package/@rustrak/mcp">
      <img src="https://img.shields.io/npm/dw/@rustrak/mcp?style=flat-square" alt="weekly downloads" />
    </a>
    <a href="https://github.com/rustrak/rustrak/blob/main/LICENSE">
      <img src="https://img.shields.io/npm/l/@rustrak/mcp?style=flat-square" alt="license" />
    </a>
    <a href="https://github.com/rustrak/rustrak/actions/workflows/ci.yml">
      <img src="https://img.shields.io/github/actions/workflow/status/rustrak/rustrak/ci.yml?style=flat-square&label=CI" alt="CI" />
    </a>
  </p>

  <p>
    <a href="https://rustrak.github.io/rustrak/sdks/mcp">Documentation</a>
    ·
    <a href="https://github.com/rustrak/rustrak">GitHub</a>
    ·
    <a href="https://github.com/rustrak/rustrak/issues">Report a Bug</a>
  </p>
</div>

---

`@rustrak/mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server for [Rustrak](https://rustrak.github.io/rustrak) — the self-hosted, Sentry-compatible error tracker. Connect it to Claude Desktop, Cursor, Continue.dev, or any MCP-compatible AI assistant and get 18 tools to list projects, inspect issues, view stack traces, resolve errors, and manage tokens without leaving your AI tool.

**[Full documentation →](https://rustrak.github.io/rustrak/sdks/mcp)**

## Quick Start

### 1. Generate an API token

In the Rustrak web UI: **Settings → Tokens → Create token**. Save the token value — it is shown only once.

### 2. Add to your AI client

**Claude Desktop** — `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "rustrak": {
      "command": "npx",
      "args": ["-y", "@rustrak/mcp"],
      "env": {
        "RUSTRAK_API_URL": "https://your-rustrak-instance.example.com",
        "RUSTRAK_API_TOKEN": "your-40-char-hex-token"
      }
    }
  }
}
```

**Cursor** — `.cursor/mcp.json` (project) or `~/.cursor/mcp.json` (global):

```json
{
  "mcpServers": {
    "rustrak": {
      "command": "npx",
      "args": ["-y", "@rustrak/mcp"],
      "env": {
        "RUSTRAK_API_URL": "https://your-rustrak-instance.example.com",
        "RUSTRAK_API_TOKEN": "your-40-char-hex-token"
      }
    }
  }
}
```

**Continue.dev** — `~/.continue/config.json`:

```json
{
  "experimental": {
    "modelContextProtocolServers": [
      {
        "transport": {
          "type": "stdio",
          "command": "npx",
          "args": ["-y", "@rustrak/mcp"],
          "env": {
            "RUSTRAK_API_URL": "https://your-rustrak-instance.example.com",
            "RUSTRAK_API_TOKEN": "your-40-char-hex-token"
          }
        }
      }
    ]
  }
}
```

### 3. Ask your AI anything

```
"List all unresolved issues in project 1"
"Show me the stack trace for issue abc-123"
"Resolve all TypeError issues from the last deployment"
"Create a CI pipeline token and give me the value"
"How many events does issue xyz have?"
```

## Environment Variables

| Variable | Required | Description |
|---|---|---|
| `RUSTRAK_API_URL` | ✅ | Base URL of your Rustrak server (e.g. `https://errors.example.com`) |
| `RUSTRAK_API_TOKEN` | ✅ | 40-char hex API token — create one in Settings → Tokens |

The server exits immediately with a clear error message if either variable is missing.

## Available Tools

The most commonly used tools are listed below. The full set is discoverable at
runtime via `tools/list`.

### Projects

| Tool | Description |
|---|---|
| `list_projects` | List all projects |
| `get_project` | Get project details including DSN |
| `create_project` | Create a new project |

### Issues

| Tool | Description |
|---|---|
| `list_issues` | List issues with filters (open / resolved / muted / all) |
| `get_issue` | Get a single issue with full details |
| `resolve_issue` | Mark an issue as resolved |
| `unresolve_issue` | Re-open a resolved issue |
| `mute_issue` | Mute an issue (silences alerts) |
| `delete_issue` | ⚠️ Permanently delete an issue and all its events |

### Events

| Tool | Description |
|---|---|
| `list_events` | List raw events for an issue (cursor pagination) |
| `get_event` | Get a single event with full Sentry envelope data |

### Tokens

| Tool | Description |
|---|---|
| `list_tokens` | List API tokens (masked) |
| `create_token` | Create a new API token (full value shown once) |
| `revoke_token` | ⚠️ Permanently revoke an API token |

### Alerts

| Tool | Description |
|---|---|
| `list_alert_channels` | List notification channels (Slack, email, webhook, custom webhook) |
| `create_alert_channel` | Create a notification channel; a custom webhook takes the JSON body template |
| `update_alert_channel` | Rename, enable or disable a channel, or replace its credentials |
| `preview_alert_template` | Render a custom webhook body against a sample alert, or get the refusal |
| `test_alert_channel` | Send a test notification to a channel |
| `list_alert_rules` | List alert rules for a project |

### Project stats

| Tool | Description |
|---|---|
| `get_error_volume` | Error events over time for a project, bucketed by severity |
| `get_project_stats` | Events, new issues and open issues, each vs the preceding window |
| `get_release_health` | Crash-free session and user rates per release |

> ⚠️ Destructive tools (`delete_issue`, `revoke_token`) are annotated with `destructiveHint` — supported clients will prompt for confirmation before executing.

## Architecture

```
AI Client (Claude Desktop / Cursor / Continue)
        │  stdio (JSON-RPC)
        ▼
┌─────────────────────┐
│   @rustrak/mcp      │  ← This package
│   ├── projects      │
│   ├── issues        │
│   ├── events        │
│   ├── tokens        │
│   └── alerts        │
└──────────┬──────────┘
           │  HTTP (Bearer token via @rustrak/client)
           ▼
┌─────────────────────┐
│   Rustrak Server    │
│   (Rust/Actix-web)  │
└─────────────────────┘
```

The server runs as a local process over stdio — no open network port, no daemon. The API token is read from environment variables and never exposed as a tool argument.

## Related Packages

| Package | Description |
|---|---|
| [`@rustrak/client`](https://www.npmjs.com/package/@rustrak/client) | TypeScript HTTP client for the Rustrak API — used internally by this package |

## What is Rustrak?

[Rustrak](https://rustrak.github.io/rustrak) is a self-hosted error tracking server written in Rust that accepts events from any Sentry SDK: point the SDK's DSN at it and nothing else changes. One container serves the API and the dashboard.

- [Getting Started](https://rustrak.github.io/rustrak/getting-started/overview)
- [Self-Hosting Guide](https://rustrak.github.io/rustrak/configuration/production)
- [GitHub](https://github.com/rustrak/rustrak)

## Development

```bash
# Build
pnpm --filter @rustrak/mcp build

# Run tests (33 tests)
pnpm --filter @rustrak/mcp test

# Type check
pnpm --filter @rustrak/mcp check-types

# Watch mode
pnpm --filter @rustrak/mcp dev
```

## License

[GPL-3.0](https://github.com/rustrak/rustrak/blob/main/LICENSE)
