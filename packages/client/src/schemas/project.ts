import { z } from 'zod';
import { dateTimeSchema, uuidSchema } from './common.js';

const metricDeltaSchema = z.object({
  current: z.number().int(),
  /**
   * The same window immediately before this one. Null only for all-time
   * requests, where a `0` would render as a misleading "+100%".
   */
  previous: z.number().int().nullable(),
});

/**
 * Per-row aggregates for the project list table.
 *
 * Only present when the request passed `stats_period`.
 */
export const projectListStatsSchema = z.object({
  /**
   * Distinct issues active in each bucket, oldest first. A shape, not a
   * total: an issue firing all day appears in every bucket it touched.
   */
  trend: z.array(z.number().int()),
  events: metricDeltaSchema,
  /** Issues first seen in the window. Drives the "getting worse" signal. */
  new_issues: metricDeltaSchema,
  /** Unresolved issues right now, independent of the window. */
  open_issues: z.number().int(),
  /** The subset of `open_issues` at `fatal` level. */
  fatal_issues: z.number().int(),
});

/**
 * Project response schema from API
 */
export const projectSchema = z.object({
  id: z.number().int(),
  name: z.string(),
  slug: z.string(),
  sentry_key: uuidSchema,
  dsn: z.string(),
  stored_event_count: z.number().int(),
  digested_event_count: z.number().int(),
  /** Accepted events the quota dropped instead of storing. */
  rate_limited_event_count: z.number().int(),
  /**
   * The project's own limits, or null where it follows the server's. They can
   * only tighten `MAX_EVENTS_PER_PROJECT_*`, never raise it.
   */
  rate_limit_per_minute: z.number().int().nullable(),
  rate_limit_per_hour: z.number().int().nullable(),
  created_at: dateTimeSchema,
  updated_at: dateTimeSchema,
  platform: z.string().nullable(),
  stats: projectListStatsSchema.optional(),
});

/**
 * Create project request schema
 */
export const createProjectSchema = z.object({
  name: z.string().min(1),
  /**
   * Slugified server-side. Supplying it means the user chose it, so a taken
   * slug is a 409. Omitting it derives one from the name and silently
   * de-duplicates, since nobody chose that value.
   */
  slug: z.string().min(1).optional(),
  /**
   * Validated server-side against SELECTABLE_PLATFORMS. Omitting it leaves the
   * project eligible for platform auto-detection from its first event.
   */
  platform: z.string().min(1).optional(),
});

/**
 * Update project request schema
 */
export const updateProjectSchema = z.object({
  name: z.string().min(1).optional(),
  platform: z.string().min(1).optional(),
  /**
   * Slugified server-side. Unlike on create, a taken slug is a 409 rather than
   * being silently de-duplicated.
   */
  slug: z.string().min(1).optional(),
  /** A number sets the project's own limit; null removes it. */
  rate_limit_per_minute: z.number().int().min(1).nullable().optional(),
  rate_limit_per_hour: z.number().int().min(1).nullable().optional(),
});

/**
 * The server's per-project limits (`MAX_EVENTS_PER_PROJECT_*`): what a project
 * with no limits of its own follows.
 */
export const rateLimitsSchema = z.object({
  project_per_minute: z.number().int(),
  project_per_hour: z.number().int(),
});
