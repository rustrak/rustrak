/**
 * Emits structured logs through the REAL @sentry/node SDK so we exercise the
 * exact wire format the SDK produces (the `log` item container), not a
 * hand-rolled envelope. This is the true compatibility test: if Rustrak stores
 * what `@sentry/node` sends, a Sentry SDK can't tell the difference.
 *
 * Logs are emitted via `Sentry.logger.*`.
 * The SDK batches them and flushes on `Sentry.flush()`.
 *
 * Usage:
 *   SENTRY_DSN=http://<sentry_key>@localhost:8080/<project_id> pnpm demo:logs
 * Default DSN targets project 1 on a local server.
 */
import * as Sentry from '@sentry/node';
import { DSN, ENV, RELEASE } from '../../src/env.js';

Sentry.init({
  dsn: DSN,
  release: RELEASE,
  environment: ENV,
  // Tracing on so logs emitted inside a span pick up trace_id/span_id.
  tracesSampleRate: 1.0,
  defaultIntegrations: false,
  integrations: [Sentry.contextLinesIntegration()],
});

const { logger } = Sentry;

async function main() {
  console.log(`Emitting structured logs via @sentry/node to ${DSN}\n`);

  // Standalone logs across every level, with typed attributes.
  logger.trace('Entering checkout state machine', { step: 1, state: 'cart' });
  logger.debug('Cache miss for key products:popular', {
    key: 'products:popular',
    ttlSec: 60,
    hit: false,
  });
  logger.debug('Feature flag evaluated', {
    flag: 'new_checkout',
    enabled: true,
    bucket: 0.42,
  });
  logger.info('User signed up', {
    userId: 4172,
    plan: 'pro',
    referrer: 'google',
  });
  logger.warn('Payment gateway latency above threshold', {
    gateway: 'stripe',
    latencyMs: 1830.5,
    thresholdMs: 1000,
  });
  logger.error('Failed to charge card: insufficient_funds', {
    userId: 88,
    gateway: 'stripe',
    code: 'insufficient_funds',
    amount: 59.0,
    retried: true,
  });
  logger.fatal('Database connection pool exhausted', {
    pool: 'primary',
    maxConnections: 20,
    waiting: 57,
  });

  // Logs emitted inside a span correlate to one trace (trace_id + span_id),
  // so the "filter by trace" path in the UI has data to work with.
  await Sentry.startSpan(
    { name: 'POST /api/checkout', op: 'http.server' },
    async () => {
      logger.info('Order placed', {
        orderId: 9182,
        total: 149.99,
        currency: 'EUR',
        items: 3,
      });
      logger.info('Welcome email queued', {
        userId: 4172,
        template: 'welcome_v3',
        provider: 'resend',
      });
    },
  );

  // Force the batched logs to flush before the process exits.
  await Sentry.flush(2000);
  console.log('✓ Flushed. Open the Logs tab to see them.');
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
