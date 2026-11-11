import type {
  CreateMonitor,
  MonitorConfigInput,
  MonitorResponse,
  Schedule,
} from '@rustrak/client';
import { z } from 'zod';
import type { Translate } from '@/shared/lib/error-copy';

/** Server limits (apps/server/src/models/monitor.rs). */
const MAX_MARGIN = 40_320;
const MAX_RUNTIME = 10_080;
const MAX_THRESHOLD = 720;

/** What the monitor form collects. */
export function monitorFormSchema(t: Translate) {
  // A cleared number input is NaN; it gets the same message as one out of range.
  const count = (max: number) => {
    const error = t('validation.numberRange', { min: 1, max });
    return z
      .number({ error })
      .int({ error })
      .min(1, { error })
      .max(max, { error });
  };
  return z.object({
    name: z
      .string()
      .trim()
      .min(1, t('validation.nameRequired'))
      .max(128, t('validation.tooLong', { max: 128 })),
    slug: z
      .string()
      .trim()
      .max(50, t('validation.tooLong', { max: 50 }))
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
    timezone: z.string().min(1, t('validation.timezoneRequired')),
    checkin_margin: count(MAX_MARGIN),
    max_runtime: count(MAX_RUNTIME),
    failure_issue_threshold: count(MAX_THRESHOLD),
    recovery_threshold: count(MAX_THRESHOLD),
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

export function formFromMonitor(monitor: MonitorResponse): MonitorFormData {
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
