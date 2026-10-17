import { z } from 'zod';
import { dateTimeSchema } from './common.js';

/**
 * When something recurs: a five-field crontab or a fixed interval. One shape
 * for cron monitors and for the server's own scheduled jobs, and the shape
 * Sentry SDKs send as `monitor_config.schedule`.
 */
export const intervalUnitSchema = z.enum([
  'minute',
  'hour',
  'day',
  'week',
  'month',
  'year',
]);

export const scheduleSchema = z.discriminatedUnion('type', [
  z.object({ type: z.literal('crontab'), value: z.string() }),
  z.object({
    type: z.literal('interval'),
    value: z.number().int().min(1),
    unit: intervalUnitSchema,
  }),
]);

export const previewScheduleSchema = z.object({
  schedule: scheduleSchema,
  /** IANA zone the schedule is read in. Defaults to UTC. */
  timezone: z.string().optional(),
});

/** The next runs of a schedule, from the server's own parser. */
export const schedulePreviewSchema = z.object({
  next_runs: z.array(dateTimeSchema),
});
