import type {
  CreateMonitor,
  Monitor,
  MonitorConfigInput,
  Schedule,
} from '@rustrak/client';
import { z } from 'zod';
import type { Translate } from '@/shared/lib/error-copy';

/** Server limits (apps/server/src/models/monitor.rs). */
export const MAX_MARGIN = 40_320;
export const MAX_RUNTIME = 10_080;
export const MAX_THRESHOLD = 720;

/** What the monitor form collects. */
export function monitorFormSchema(t: Translate) {
  return z.object({
    name: z.string().trim().min(1, t('validation.nameRequired')).max(128),
    slug: z
      .string()
      .trim()
      .max(50)
      .regex(/^[a-z0-9_-]*$/, t('validation.slugFormat')),
    // Whether a crontab parses is the server's call (`schedules.preview`).
    schedule: z.discriminatedUnion('type', [
      z.object({
        type: z.literal('crontab'),
        value: z.string().trim().min(1, t('validation.crontabInvalid')),
      }),
      z.object({
        type: z.literal('interval'),
        value: z.number().int().min(1, t('validation.intervalMin')),
        unit: z.enum(['minute', 'hour', 'day', 'week', 'month', 'year']),
      }),
    ]),
    timezone: z.string().min(1),
    checkin_margin: z.number().int().min(1).max(MAX_MARGIN),
    max_runtime: z.number().int().min(1).max(MAX_RUNTIME),
    failure_issue_threshold: z.number().int().min(1).max(MAX_THRESHOLD),
    recovery_threshold: z.number().int().min(1).max(MAX_THRESHOLD),
  });
}

export type MonitorFormData = z.infer<ReturnType<typeof monitorFormSchema>>;

/** Sentry's defaults and an hourly schedule. */
export const DEFAULT_MONITOR_FORM: MonitorFormData = {
  name: '',
  slug: '',
  schedule: { type: 'crontab', value: '0 * * * *' },
  timezone: 'UTC',
  checkin_margin: 1,
  max_runtime: 30,
  failure_issue_threshold: 1,
  recovery_threshold: 1,
};

export function formFromMonitor(monitor: Monitor): MonitorFormData {
  return { ...monitor.config, name: monitor.name, slug: monitor.slug };
}

function trimmed(schedule: Schedule): Schedule {
  return schedule.type === 'crontab'
    ? { type: 'crontab', value: schedule.value.trim() }
    : schedule;
}

export function inputFromForm(
  data: MonitorFormData,
): CreateMonitor & { config: MonitorConfigInput } {
  const slug = data.slug.trim();
  return {
    name: data.name.trim(),
    ...(slug ? { slug } : {}),
    config: {
      schedule: trimmed(data.schedule),
      checkin_margin: data.checkin_margin,
      max_runtime: data.max_runtime,
      timezone: data.timezone,
      failure_issue_threshold: data.failure_issue_threshold,
      recovery_threshold: data.recovery_threshold,
    },
  };
}
