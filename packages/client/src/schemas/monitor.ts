import { z } from 'zod';
import { dateTimeSchema, uuidSchema } from './common.js';
import { scheduleSchema } from './schedule.js';

/**
 * Cron monitors (Sentry Crons). The schedule shape is Relay's
 * `monitor_config.schedule`, so the same object an SDK upserts with is what
 * the dashboard reads and writes.
 */
/** A validated config, every default filled in. Minutes for durations. */
export const monitorConfigSchema = z.object({
  schedule: scheduleSchema,
  checkin_margin: z.number().int(),
  max_runtime: z.number().int(),
  timezone: z.string(),
  failure_issue_threshold: z.number().int(),
  recovery_threshold: z.number().int(),
});

/** A config as sent: fields left out keep their value (or the default). */
export const monitorConfigInputSchema = z.object({
  schedule: scheduleSchema.optional(),
  checkin_margin: z.number().int().min(1).optional(),
  max_runtime: z.number().int().min(1).optional(),
  timezone: z.string().optional(),
  failure_issue_threshold: z.number().int().min(1).max(720).optional(),
  recovery_threshold: z.number().int().min(1).max(720).optional(),
});

/** `missed` and `timeout` are only ever set by the server. */
export const checkInStatusSchema = z.enum([
  'ok',
  'error',
  'in_progress',
  'missed',
  'timeout',
  'unknown',
]);

/** `active` waits for a first ok check-in; `error` means an incident is open. */
export const monitorEnvironmentStatusSchema = z.enum(['active', 'ok', 'error']);

export const checkInSchema = z.object({
  id: uuidSchema,
  environment: z.string(),
  status: checkInStatusSchema,
  duration_ms: z.number().int().nullable(),
  date_added: dateTimeSchema,
  date_updated: dateTimeSchema,
  expected_time: dateTimeSchema.nullable(),
});

export const monitorEnvironmentSchema = z.object({
  id: uuidSchema,
  environment: z.string(),
  status: monitorEnvironmentStatusSchema,
  next_checkin: dateTimeSchema.nullable(),
  next_checkin_latest: dateTimeSchema.nullable(),
  last_checkin: dateTimeSchema.nullable(),
  /** Newest first. */
  recent_checkins: z.array(checkInSchema),
});

export const monitorSchema = z.object({
  id: uuidSchema,
  slug: z.string(),
  name: z.string(),
  config: monitorConfigSchema,
  is_muted: z.boolean(),
  created_at: dateTimeSchema,
  environments: z.array(monitorEnvironmentSchema),
});

export const createMonitorSchema = z.object({
  name: z.string().trim().min(1).max(128),
  /** Derived from `name` when left out. */
  slug: z.string().min(1).max(50).optional(),
  config: monitorConfigInputSchema,
});

export const updateMonitorSchema = z.object({
  name: z.string().trim().min(1).max(128).optional(),
  config: monitorConfigInputSchema.optional(),
  is_muted: z.boolean().optional(),
});
