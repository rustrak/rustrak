/**
 * Sentry must be initialized — with `vercelAIIntegration()` registered —
 * before the `ai` package is ever imported, so OpenTelemetry's ESM module
 * hook can patch `generateText`/`streamText`/etc. Node only guarantees that
 * ordering via `--import` preload (Sentry's own documented pattern for ESM
 * apps), which is why this lives in its own file instead of at the top of
 * ai-agent.ts.
 *
 * Usage: tsx --import demo/src/ai-agent-instrument.ts demo/src/ai-agent.ts
 * (see the `demo:ai-agent` script in package.json)
 */
import * as Sentry from '@sentry/node';
import { DSN, ENV, RELEASE } from '../../src/env.js';

Sentry.init({
  dsn: DSN,
  release: RELEASE,
  environment: ENV,
  tracesSampleRate: 1.0,
  debug: process.env.SENTRY_DEBUG === '1',
  defaultIntegrations: false,
  integrations: [
    Sentry.contextLinesIntegration(),
    Sentry.vercelAIIntegration(),
  ],
});
