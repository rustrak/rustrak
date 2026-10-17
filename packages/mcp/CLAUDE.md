# @rustrak/mcp

An MCP server that wraps `@rustrak/client` and exposes Rustrak as tools, so an
assistant can list projects, inspect issues and stack traces, resolve errors,
manage tokens and administer the team without leaving the tool it runs in.
Root context: `/CLAUDE.md`.

Runs over stdio as a local process, so there is no network port to open. The API
token is read from the environment and never accepted as a tool argument.

## Layout

```
src/
├── index.ts     entry: loadConfig, build a RustrakClient, serve over stdio
├── server.ts    createServer(client) returns the McpServer. Injected, so tests
│                can drive it in-process over InMemoryTransport.
├── config.ts    loadConfig: validates RUSTRAK_API_URL and RUSTRAK_API_TOKEN
├── errors.ts    mcpJson, mcpDone, mcpRefusal and toMcpError
└── tools/       one module per area: projects, issues, events, transactions,
                 spans, agents, sessions, logs, monitors, schedules, stats, storage, team, tokens,
                 alerts, health
```

## Adding a tool

1. Put it in the module for its area, registered with `.registerTool()`.
2. Describe the arguments with Zod. The SDK reads them as Standard Schema.
3. Return through the helpers in `errors.ts`. Never throw: an API failure comes
   back as `isError: true` content, because a thrown error kills the session
   instead of telling the assistant what went wrong.
4. Annotate anything destructive with `destructiveHint` so the client can
   confirm before running it.
5. Cover it in `tests/`, driving the real server over `InMemoryTransport`.

Since `@rustrak/client` never throws either, a tool body is normally a `Result`
check and a formatted response. If you find yourself writing `try`/`catch`, that
is the signal something is bypassing the client.

## Configuration

| Variable | Meaning |
|---|---|
| `RUSTRAK_API_URL` | Base URL of the Rustrak server |
| `RUSTRAK_API_TOKEN` | Bearer token created at `/settings/tokens` |

Both are required and validated at startup, so a misconfiguration fails
immediately rather than on the first tool call.

## Build and test

```bash
pnpm --filter=@rustrak/mcp build     # ESM only, "type": "module", bin rustrak-mcp
pnpm --filter=@rustrak/mcp test
```

Tests run in-process with no subprocess, which keeps them fast enough to run on
every change.
