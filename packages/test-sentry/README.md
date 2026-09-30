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

# Run all tests
pnpm start --dsn "http://<sentry_key>@localhost:8080/1" --all

# Run a specific test
pnpm start --dsn "http://<sentry_key>@localhost:8080/1" --error

# Test rate limiting with many events
pnpm start --dsn "http://<sentry_key>@localhost:8080/1" --flood 100

# Enable debug mode to see Sentry SDK output
pnpm start --dsn "http://<sentry_key>@localhost:8080/1" --all --debug
```

### Using environment variable

```bash
export SENTRY_DSN="http://<sentry_key>@localhost:8080/1"
pnpm start --all
```

### Compare environments

Send both runs to the same project DSN. Each run adds events; it does not clear earlier data.

```bash
pnpm start --dsn "http://<sentry_key>@localhost:8080/1" --all --env production
pnpm start --dsn "http://<sentry_key>@localhost:8080/1" --all --env staging
```

Open that project in the dashboard and switch between All environments, production, and staging.

## Available Tests

| Flag | Description |
|------|-------------|
| `--all` | Run all tests |
| `--error` | Basic captured exception |
| `--type-error` | TypeError exception |
| `--ref-error` | ReferenceError exception |
| `--custom-error` | Custom error class with extra properties |
| `--nested` | Nested error with cause chain |
| `--async` | Async error with stack trace |
| `--message` | Capture message (non-error event) |
| `--breadcrumbs` | Error with breadcrumb trail |
| `--user` | Error with user context |
| `--tags` | Error with custom tags |
| `--context` | Error with extra context data |
| `--contexts` | Error with rich contexts (device, os, app) |
| `--fingerprint` | Error with custom fingerprint for grouping |
| `--transaction` | Error with transaction name |
| `--levels` | Test all severity levels (fatal, error, warning, info, debug) |
| `--multiple` | Multiple sequential errors |
| `--flood [count]` | Send many errors to test rate limiting (default: 50) |

## Configuration Options

| Flag | Description |
|------|-------------|
| `--dsn <dsn>` | Sentry/Rustrak DSN (required) |
| `--debug` | Enable Sentry SDK debug mode |
| `--env <env>` | Set environment (default: test) |
| `--release <rel>` | Set release version |

## NPM Scripts

```bash
# Run with CLI arguments
pnpm start [options]

# Shortcuts for common tests
pnpm test:all           # Run all tests
pnpm test:error         # Basic error
pnpm test:breadcrumbs   # Breadcrumbs test
pnpm test:flood         # Rate limit test
# ... etc
```

Note: You need to set `SENTRY_DSN` env var or modify the scripts to include your DSN.

## Programmatic Usage

You can also use the test functions programmatically:

```typescript
import {
  initSentry,
  testCapturedError,
  testWithBreadcrumbs,
  flush,
} from '@rustrak/test-sentry';

// Initialize
initSentry({
  dsn: 'http://key@localhost:8080/1',
  debug: true,
});

// Run tests
testCapturedError();
testWithBreadcrumbs();

// Wait for events to be sent
await flush(5000);
```

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
