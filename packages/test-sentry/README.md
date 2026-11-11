# @rustrak/test-sentry

Test utility for sending Sentry events to a Rustrak server. This package allows you to test all aspects of the Sentry protocol implementation.

## Installation

From the monorepo root:

```bash
pnpm install
```

## Usage

### Get your DSN

First, you need a DSN from your Rustrak server. You can find it in the project settings or when creating a new project. The format is:

```
http://<sentry_key>@<host>:<port>/<project_id>
```

Example: `http://a1b2c3d4-e5f6-7890-abcd-ef1234567890@localhost:8080/1`

### Run tests

```bash
cd packages/test-sentry

pnpm start --all                                   # every scenario but flood and source maps
pnpm start --error                                 # one scenario
pnpm start --flood 100                             # rate limiting
pnpm start --dsn "http://<sentry_key>@host:8080/2" --all --debug
npx tsx src/cli.ts --help                          # every flag (pnpm keeps --help for itself)
```

The DSN defaults to `SENTRY_DSN`, else project 1 on a local server
(`src/env.ts`); the demos under `demo/` and `send-transactions.ts` use the same
default.

## What Each Test Validates

### Error Tests
- **Basic Error**: Verifies the server receives and stores a simple exception
- **TypeError/ReferenceError**: Tests common JavaScript error types
- **Custom Error**: Tests that custom error classes with extra properties are captured
- **Nested Error**: Tests error cause chains (Error.cause)
- **Async Error**: Tests async stack traces

### Context Tests
- **Breadcrumbs**: Verifies breadcrumb trail is attached to events
- **User Context**: Tests user information (id, email, username)
- **Tags**: Tests custom tags are stored and filterable
- **Extra Context**: Tests arbitrary context data
- **Rich Contexts**: Tests device, os, app contexts

### Grouping Tests
- **Fingerprint**: Verifies custom fingerprints affect issue grouping
- **Transaction**: Tests transaction name affects grouping

### Stress Tests
- **Multiple**: Sends several events sequentially
- **Flood**: Sends many events to test rate limiting
