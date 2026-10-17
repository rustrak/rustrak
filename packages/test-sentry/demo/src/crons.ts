/**
 * Checks in through the REAL @sentry/node SDK, so Rustrak is tested against
 * the `check_in` items the SDK actually sends rather than hand-built ones.
 *
 * Three monitors:
 *   - `demo-backup`: `withMonitor` with a `monitorConfig` (an upsert), a run
 *     that succeeds: in_progress then ok.
 *   - `demo-report`: an interval monitor whose run throws: in_progress then
 *     error, which opens an issue.
 *   - `demo-sync`: `captureCheckIn` in_progress with a 1 minute max runtime
 *     and never finished, which the sweep times out a minute later.
 *
 * Usage:
 *   SENTRY_DSN=http://<sentry_key>@localhost:8080/<project_id> pnpm demo:crons
 */
import * as Sentry from '@sentry/node';

const DSN =
  process.env.SENTRY_DSN ??
  'http://0cabb1ed295f4cf6b41411220caa8eb6@localhost:8080/1';

Sentry.init({ dsn: DSN, defaultIntegrations: false });

await Sentry.withMonitor(
  'demo-backup',
  async () => {
    await new Promise((resolve) => setTimeout(resolve, 1200));
  },
  {
    schedule: { type: 'crontab', value: '0 3 * * *' },
    checkinMargin: 5,
    maxRuntime: 30,
    timezone: 'Europe/Madrid',
  },
);

try {
  await Sentry.withMonitor(
    'demo-report',
    async () => {
      throw new Error('report generation failed');
    },
    { schedule: { type: 'interval', value: 1, unit: 'hour' } },
  );
} catch {
  // Expected: the SDK reports the run as failed and rethrows.
}

Sentry.captureCheckIn(
  { monitorSlug: 'demo-sync', status: 'in_progress' },
  { schedule: { type: 'crontab', value: '*/5 * * * *' }, maxRuntime: 1 },
);

await Sentry.flush(5000);
console.log('Sent check-ins for demo-backup, demo-report and demo-sync.');
