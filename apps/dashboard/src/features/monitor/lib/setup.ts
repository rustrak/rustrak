import type { Schedule } from '@rustrak/client';

/**
 * How to make a job check in, per tool.
 *
 * Taken from Sentry's `views/insights/crons/components/manualCheckInGuides.tsx`
 * and `upsertPlatformGuides.tsx` at commit 2458718b, with the DSN and slug
 * filled in. Given a schedule, each guide also sends `monitor_config`, which
 * creates the monitor on its first check-in (an upsert); without one it
 * checks in to a monitor that already exists.
 */
export const CRON_GUIDES = [
  'curl',
  'cli',
  'python',
  'node',
  'laravel',
] as const;
export type CronGuideId = (typeof CRON_GUIDES)[number];

export interface CronGuide {
  /** Prism language id. */
  language: string;
  code: string;
}

interface GuideInput {
  dsn: string;
  slug: string;
  schedule?: Schedule;
}

/** The parts of a DSN: `<protocol>://<key>@<host>[/<prefix>]/<project_id>`. */
export function parseDsn(
  dsn: string,
): { origin: string; key: string; projectId: string } | null {
  let url: URL;
  try {
    url = new URL(dsn);
  } catch {
    return null;
  }
  const segments = url.pathname.split('/').filter(Boolean);
  const projectId = segments.pop();
  if (!url.username || !projectId) return null;
  const prefix = segments.length ? `/${segments.join('/')}` : '';
  return {
    origin: `${url.protocol}//${url.host}${prefix}`,
    key: url.username,
    projectId,
  };
}

/** The HTTP check-in endpoint, with the key in the path as Relay accepts it. */
export function cronCheckInUrl(dsn: string, slug: string): string | null {
  const parts = parseDsn(dsn);
  if (!parts) return null;
  return `${parts.origin}/api/${parts.projectId}/cron/${slug}/${parts.key}/`;
}

function jsonSchedule(schedule: Schedule): string {
  return schedule.type === 'crontab'
    ? `{"type": "crontab", "value": "${schedule.value}"}`
    : `{"type": "interval", "value": ${schedule.value}, "unit": "${schedule.unit}"}`;
}

function curl({ dsn, slug, schedule }: GuideInput): string {
  const url = cronCheckInUrl(dsn, slug) ?? '<check-in url>';
  const start = schedule
    ? `# 🟡 Notify Rustrak your job is running (and create the monitor):
curl -X POST "$RUSTRAK_CRONS" \\
    --header 'Content-Type: application/json' \\
    --data-raw '{
      "monitor_config": {
        "schedule": ${jsonSchedule(schedule)}
      },
      "status": "in_progress"
    }'`
    : `# 🟡 Notify Rustrak your job is running:
curl "$RUSTRAK_CRONS?status=in_progress"`;
  return `RUSTRAK_CRONS="${url}"

${start}

# Execute your scheduled task here...

# 🟢 Notify Rustrak your job has completed successfully:
curl "$RUSTRAK_CRONS?status=ok"

# 🔴 Or notify Rustrak your job has failed:
curl "$RUSTRAK_CRONS?status=error"`;
}

function cli({ dsn, slug, schedule }: GuideInput): string {
  // sentry-cli only takes a crontab; an interval monitor checks in without
  // upserting.
  const upsert =
    schedule?.type === 'crontab' ? ` --schedule "${schedule.value}"` : '';
  return `# Wraps the command: in_progress before, ok or error from its exit code.
export SENTRY_DSN=${dsn}
sentry-cli monitors run${upsert} ${slug} -- python path/to/file`;
}

function python({ slug, schedule }: GuideInput): string {
  if (!schedule) {
    return `from sentry_sdk.crons import monitor

# Add this decorator to instrument your python function
@monitor(monitor_slug='${slug}')
def tell_the_world(msg):
    print(msg)`;
  }
  const config =
    schedule.type === 'crontab'
      ? `{"type": "crontab", "value": "${schedule.value}"}`
      : `{"type": "interval", "value": ${schedule.value}, "unit": "${schedule.unit}"}`;
  return `from sentry_sdk.crons import monitor

# All keys except 'schedule' are optional
monitor_config = {
    "schedule": ${config},
    "checkin_margin": 5,
    "max_runtime": 10,
}

@monitor(monitor_slug='${slug}', monitor_config=monitor_config)
def tell_the_world():
    print('My scheduled task...')`;
}

function node({ slug, schedule }: GuideInput): string {
  if (!schedule) {
    return `Sentry.withMonitor("${slug}", () => {
  // Execute your scheduled task here...
});`;
  }
  const shape =
    schedule.type === 'crontab'
      ? `type: "crontab",\n    value: "${schedule.value}",`
      : `type: "interval",\n    value: ${schedule.value},\n    unit: "${schedule.unit}",`;
  return `const monitorConfig = {
  schedule: {
    ${shape}
  },
  checkinMargin: 5, // In minutes. Optional.
  maxRuntime: 10, // In minutes. Optional.
};

Sentry.withMonitor(
  "${slug}",
  () => {
    // Execute your scheduled task here...
  },
  monitorConfig
);`;
}

function laravel({ slug }: GuideInput): string {
  // The Laravel SDK derives the schedule from the task itself.
  return `Schedule::command(SendEmailsCommand::class)
    ->everyHour()
    ->sentryMonitor(monitorSlug: '${slug}'); // add this line`;
}

const BUILDERS: Record<CronGuideId, [string, (input: GuideInput) => string]> = {
  curl: ['bash', curl],
  cli: ['bash', cli],
  python: ['python', python],
  node: ['javascript', node],
  laravel: ['php', laravel],
};

export function cronGuide(id: CronGuideId, input: GuideInput): CronGuide {
  const [language, build] = BUILDERS[id];
  return { language, code: build(input) };
}
