import * as Sentry from '@sentry/node';
import { DSN, ENV, RELEASE } from '../../src/env.js';

Sentry.init({
  dsn: DSN,
  release: RELEASE,
  environment: ENV,
  tracesSampleRate: 1.0,
  defaultIntegrations: false,
  integrations: [
    Sentry.consoleIntegration(),
    Sentry.httpIntegration(),
    Sentry.contextLinesIntegration(),
  ],
});

async function sleep(ms: number) {
  return new Promise((r) => setTimeout(r, ms));
}

function bulkSessions() {
  for (const release of [
    'frontend@2.0.0',
    'frontend@2.0.1',
    'frontend@2.1.0',
  ]) {
    for (let i = 0; i < 3; i++) {
      Sentry.startSession({
        sid: `sess-bulk-${release}-${i}`,
        did: `user-bulk-${i}`,
        status: 'ok',
        errors: i === 1 ? 2 : 0,
      });
      if (i === 0) {
        try {
          throw new Error(`Crash on ${release}`);
        } catch (err) {
          Sentry.captureException(err);
        }
      }
      Sentry.endSession();
    }
  }
}

async function main() {
  console.log('=== Session Tracking Demo ===\n');
  console.log(`DSN: ${DSN}`);
  console.log(`Release: ${RELEASE}`);
  console.log(`Environment: ${ENV}\n`);

  // -----------------------------------------------------------------------
  // Scenario 1: Healthy session — user browses, no errors
  // -----------------------------------------------------------------------
  console.log('--- Scenario 1: Healthy session ---');
  Sentry.startSession({
    sid: 'sess-healthy-1',
    did: 'user-alice',
    status: 'ok',
    errors: 0,
  });
  await sleep(500);
  Sentry.captureMessage('Page loaded: /dashboard');
  await sleep(300);
  Sentry.captureMessage('Page loaded: /settings');
  await sleep(300);
  Sentry.endSession();
  console.log('  Healthy session ended (ok)\n');

  // -----------------------------------------------------------------------
  // Scenario 2: Errored session — user hits an error but continues
  // -----------------------------------------------------------------------
  console.log('--- Scenario 2: Errored session ---');
  Sentry.startSession({
    sid: 'sess-errored-1',
    did: 'user-bob',
    status: 'ok',
    errors: 0,
  });
  await sleep(200);
  try {
    throw new Error('Failed to fetch user profile');
  } catch (err) {
    Sentry.captureException(err);
  }
  await sleep(200);
  Sentry.captureMessage('User retried and succeeded');
  await sleep(200);
  Sentry.endSession();
  console.log('  Errored session ended (ok with errors)\n');

  // -----------------------------------------------------------------------
  // Scenario 3: Crashed session — user hits a fatal error
  // -----------------------------------------------------------------------
  console.log('--- Scenario 3: Crashed session ---');
  Sentry.startSession({
    sid: 'sess-crashed-1',
    did: 'user-charlie',
    status: 'ok',
    errors: 0,
  });
  await sleep(200);
  try {
    throw new Error('Out of memory');
  } catch (err) {
    Sentry.captureException(err, { mechanism: { handled: false } });
  }
  await sleep(200);
  Sentry.endSession();
  console.log('  Crashed session ended (crashed)\n');

  // -----------------------------------------------------------------------
  // Scenario 4: Multiple users on different releases
  // -----------------------------------------------------------------------
  console.log('--- Scenario 4: Multiple users, multiple releases ---');

  bulkSessions();
  console.log('  Bulk sessions sent (3 releases × 3 users)\n');

  // -----------------------------------------------------------------------
  // Flush everything
  // -----------------------------------------------------------------------
  console.log('--- Flushing ---');
  const flushed = await Sentry.flush(10000);
  console.log(`Flush: ${flushed ? 'success' : 'timeout'}`);
  console.log('\n=== Done ===');
  console.log('Check the Rustrak dashboard → Project → Release Health');
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
