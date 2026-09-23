<div align="center">
  <a href="https://rustrak.github.io/rustrak">
    <img src="https://raw.githubusercontent.com/rustrak/rustrak/main/apps/docs/public/logo.svg" alt="Rustrak" width="64" height="64" />
  </a>
  <h1>@rustrak/client</h1>
  <p>Official TypeScript client for the <a href="https://rustrak.github.io/rustrak">Rustrak</a> self-hosted error tracking API</p>

  <p>
    <a href="https://www.npmjs.com/package/@rustrak/client">
      <img src="https://img.shields.io/npm/v/@rustrak/client?style=flat-square&color=cb3837" alt="npm version" />
    </a>
    <a href="https://www.npmjs.com/package/@rustrak/client">
      <img src="https://img.shields.io/npm/dw/@rustrak/client?style=flat-square" alt="weekly downloads" />
    </a>
    <a href="https://bundlephobia.com/package/@rustrak/client">
      <img src="https://img.shields.io/bundlephobia/minzip/@rustrak/client?style=flat-square&label=bundle" alt="bundle size" />
    </a>
    <a href="https://github.com/rustrak/rustrak/blob/main/LICENSE">
      <img src="https://img.shields.io/npm/l/@rustrak/client?style=flat-square" alt="license" />
    </a>
    <a href="https://github.com/rustrak/rustrak/actions/workflows/ci.yml">
      <img src="https://img.shields.io/github/actions/workflow/status/rustrak/rustrak/ci.yml?style=flat-square&label=CI" alt="CI" />
    </a>
  </p>

  <p>
    <a href="https://rustrak.github.io/rustrak/sdks/client">Documentation</a>
    ·
    <a href="https://github.com/rustrak/rustrak">GitHub</a>
    ·
    <a href="https://github.com/rustrak/rustrak/issues">Report a Bug</a>
  </p>
</div>

---

`@rustrak/client` is the official TypeScript client for [Rustrak](https://rustrak.github.io/rustrak) — an ultra-lightweight, self-hosted error tracking system compatible with any Sentry SDK. This package wraps the Rustrak REST API with full type safety, runtime validation via Zod, built-in retry logic, and structured error handling. Total bundle size: ~28 KB.

## Installation

```bash
npm install @rustrak/client
# or
pnpm add @rustrak/client
# or
yarn add @rustrak/client
```

**Requirements**: Node.js ≥ 18, TypeScript ≥ 5

## Quick Start

```typescript
import { RustrakClient } from '@rustrak/client';

const client = new RustrakClient({
  baseUrl: 'https://your-rustrak-instance.example.com',
  token: process.env.RUSTRAK_API_TOKEN!,
});

// Every method returns a Result. Nothing throws for an expected failure.
const projects = await client.projects.list();
if (!projects.success) {
  console.error(projects.error.kind, projects.error.message);
  return;
}
console.log(projects.data.items);

// Paginate through open issues
const issues = await client.issues.list(1, { sort: 'last_seen', order: 'desc' });
if (issues.success) {
  const { items, next_cursor, has_more } = issues.data;
}

// Resolve an issue
const resolved = await client.issues.updateState(1, 'issue-id', {
  is_resolved: true,
});
```

**[Full documentation →](https://rustrak.github.io/rustrak/sdks/client)**

## Features

- **Type-safe** — All API responses validated at runtime with Zod; types are inferred from schemas
- **Lightweight** — ~28 KB total (ky 3 KB + zod 10 KB + client 15 KB)
- **Automatic retry** — Exponential backoff on transient failures (408, 429, 5xx)
- **No exceptions**: every method returns `Result<T, RustrakError>`, so failure is in the type, and the value survives the React server/client boundary
- **Cursor pagination** — First-class support for paginated responses across all list endpoints
- **Redacted 5xx**: a server error never carries a server-supplied message, so internal detail cannot reach a UI
- **High test coverage**: 430+ tests (unit + integration with MSW)

## API Reference

### Configuration

```typescript
const client = new RustrakClient({
  baseUrl: 'https://rustrak.example.com', // required
  token: 'your-bearer-token',             // required
  timeout: 30000,                         // optional, ms (default: 30000)
  maxRetries: 2,                          // optional (default: 2)
  headers: {},                            // optional custom headers
});
```

### Projects

```typescript
const projects = await client.projects.list();
const project  = await client.projects.get(1);
const created  = await client.projects.create({ name: 'My App', slug: 'my-app' });
const updated  = await client.projects.update(1, { name: 'New Name' });
await client.projects.delete(1);
```

### Issues

```typescript
// List with filters and cursor pagination
const { items, next_cursor, has_more } = await client.issues.list(projectId, {
  sort: 'last_seen',        // 'digest_order' | 'last_seen'
  order: 'desc',            // 'asc' | 'desc'
  include_resolved: false,
  cursor: 'eyJzb3J0...',    // from previous response
});

const issue = await client.issues.get(projectId, issueId);
await client.issues.updateState(projectId, issueId, { is_resolved: true });
await client.issues.delete(projectId, issueId);
```

### Events

```typescript
const { items } = await client.events.list(projectId, issueId, { order: 'desc' });
const event     = await client.events.get(projectId, issueId, eventId);
// Resolve the ID returned by a Sentry SDK without knowing its issue first.
const captured  = await client.events.getBySentryId(projectId, sentryEventId);
console.log(event.data); // Full Sentry event payload
```

### Auth Tokens

```typescript
const tokens  = await client.tokens.list();
const created = await client.tokens.create({ description: 'CI token' });
if (created.success) {
  console.log(created.data.token); // Save this, shown only once
}
await client.tokens.delete(1);
```

## Error Handling

Resource methods do not throw. Each returns a `Result`, a plain discriminated
union mirroring Zod's `safeParse`:

```typescript
type Result<T, E> =
  | { success: true;  data: T }
  | { success: false; error: E };
```

Failures are one closed union keyed on `kind`, so the compiler forces you to
handle the case you care about and lets you ignore the rest deliberately:

```typescript
import { isRetryable, type RustrakError } from '@rustrak/client';

const result = await client.projects.list();

if (!result.success) {
  switch (result.error.kind) {
    case 'unauthenticated':
      redirect('/login');
      break;
    case 'rate_limited':
      console.log(`Retry after ${result.error.retryAfter ?? '?'}s`);
      break;
    case 'not_found':
      console.log('Gone');
      break;
    default:
      if (isRetryable(result.error)) scheduleRetry();
      else console.error(result.error.message);
  }
  return;
}

console.log(result.data.items);
```

### The error union

| `kind` | HTTP | `status` | Retryable | Notes |
|---|---|---|---|---|
| `validation` | 400 | ✅ | ❌ | The server rejected the request |
| `unauthenticated` | 401 | ✅ | ❌ | No session, or it expired |
| `forbidden` | 403 | ✅ | ❌ | Authenticated but not allowed |
| `not_found` | 404 | ✅ | ❌ | |
| `conflict` | 409 | ✅ | ❌ | |
| `gone` | 410 | ✅ | ❌ | |
| `payload_too_large` | 413 | ✅ | ❌ | Envelope ingestion only |
| `rate_limited` | 429 | ✅ | ✅ | Optional `retryAfter` in seconds |
| `client_error` | other 4xx | ✅ | ❌ | Catch-all, so no status is unrepresentable |
| `server_error` | 5xx | ✅ | ✅ | **Message is always a fixed generic string** |
| `network` | n/a | no | ✅ | DNS, refused connection, TLS, timeout |
| `invalid_response` | n/a | no | ❌ | 2xx whose body failed its schema |
| `invalid_request` | n/a | no | ❌ | Your input failed a pre-flight check; nothing was sent |

`status` is `number`, never a literal, so a proxy-generated 502 or a future
status is a value you can log rather than a type error.

Three things the union deliberately does **not** carry, because each embeds
information that must not cross a serialization boundary:

- `server_error` never carries the server's message. `AppError::Internal` and
  `AppError::Database` interpolate arbitrary internal text (a pool error, an OS
  errno, a filesystem path); the client discards it at construction and
  substitutes `SERVER_ERROR_MESSAGE`.
- `network` carries no `cause`. The underlying error embeds the resolved host
  and port.
- `invalid_response` carries no Zod issues. They embed the offending response
  data.

### Result helpers

```typescript
import { Ok, Err, unwrap, unwrapOr, mapResult } from '@rustrak/client';

mapResult(await client.projects.get(1), (project) => project.name);
unwrap(await client.projects.get(1)); // throws on failure: opting back in
unwrapOr(await client.stats.summary(1, "24h"), null); // see the warning below
```

> **`unwrapOr` is not for rendering.** `unwrapOr(await client.projects.list(),
> [])` shows the same empty state for "this account has no projects" and "the
> server is unreachable": the exact confusion the `Result` API exists to
> prevent, one line after the failure was produced. Use it only where the
> fallback is correct regardless of *why* the call failed (a cached-count
> optimisation, a best-effort telemetry read, a script that already logged the
> error). Anywhere a human sees the outcome, branch on `result.success` and read
> `error.kind`.

Every value in a `Result` is a plain object with `Object.prototype`. That is
what lets a failure be returned from a Server Component or a Server Action and
read in a Client Component: React's Flight serializer rejects class instances,
which is why there are no error classes here.

## Next.js Integration

### Server Component

```typescript
import { RustrakClient } from '@rustrak/client';

export default async function ProjectsPage() {
  const client = new RustrakClient({
    baseUrl: process.env.RUSTRAK_API_URL!,
    token: process.env.RUSTRAK_API_TOKEN!,
  });
  const projects = await client.projects.list();

  if (!projects.success) {
    // `kind` is what decides: only 'unauthenticated' means "send to login".
    // Redirecting on 'network' or 'server_error' turns a flaky connection
    // into a login loop.
    if (projects.error.kind === 'unauthenticated') redirect('/auth/login');
    return <LoadFailed error={projects.error} />;
  }

  return <ProjectsList projects={projects.data.items} />;
}
```

### Server Action

```typescript
'use server';
import { RustrakClient } from '@rustrak/client';

export async function resolveIssue(projectId: number, issueId: string) {
  const client = new RustrakClient({
    baseUrl: process.env.RUSTRAK_API_URL!,
    token: process.env.RUSTRAK_API_TOKEN!,
  });
  // The Result is returned as-is: it is serializable, so the client component
  // receives the failure instead of an opaque "An error occurred" digest.
  return client.issues.updateState(projectId, issueId, { is_resolved: true });
}
```

## TypeScript

All types are exported and inferred from Zod schemas — single source of truth:

```typescript
import type {
  Project,
  Issue,
  Event,
  EventDetail,
  AuthToken,
  PaginatedResponse,
  CreateProject,
  UpdateIssueState,
  Result,
  RustrakError,
  RustrakErrorKind,
} from '@rustrak/client';
```

## Related Packages

| Package | Description |
|---|---|
| [`@rustrak/mcp`](https://www.npmjs.com/package/@rustrak/mcp) | MCP server — gives Claude Desktop, Cursor, and Continue direct access to your Rustrak instance via 18 tools |

## What is Rustrak?

[Rustrak](https://rustrak.github.io/rustrak) is a self-hosted error tracking server written in Rust that is fully compatible with any Sentry SDK. Drop-in replacement for Sentry — no code changes needed. Runs on ~50 MB of memory as a single binary or Docker image.

- [Getting Started](https://rustrak.github.io/rustrak/getting-started/overview)
- [Self-Hosting Guide](https://rustrak.github.io/rustrak/configuration/production)
- [API Reference](https://rustrak.github.io/rustrak/api-reference)
- [GitHub](https://github.com/rustrak/rustrak)

## License

[GPL-3.0](https://github.com/rustrak/rustrak/blob/main/LICENSE)
