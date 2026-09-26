import { z } from 'zod';
import { dateTimeSchema, eventIdSchema, uuidSchema } from './common.js';

/**
 * Event response schema from list endpoint
 */
export const eventSchema = z.object({
  id: uuidSchema,
  event_id: eventIdSchema,
  issue_id: uuidSchema.nullable(),
  title: z.string(),
  timestamp: dateTimeSchema,
  level: z.string(),
  platform: z.string(),
  release: z.string(),
  environment: z.string(),
  event_type: z.string(),
});

/**
 * Event detail response schema from detail endpoint
 */
export const eventDetailSchema = z.object({
  id: uuidSchema,
  event_id: eventIdSchema,
  issue_id: uuidSchema.nullable(),
  title: z.string(),
  timestamp: dateTimeSchema,
  ingested_at: dateTimeSchema,
  level: z.string(),
  platform: z.string(),
  release: z.string(),
  environment: z.string(),
  server_name: z.string(),
  sdk_name: z.string(),
  sdk_version: z.string(),
  event_type: z.string(),
  data: z.record(z.string(), z.any()), // Full Sentry event JSON
});

/**
 * Where one event sits among its issue's events, oldest first
 */
export const eventNavigationSchema = z.object({
  current_index: z.number().int(),
  total_count: z.number().int(),
  first_event_id: uuidSchema.nullable(),
  last_event_id: uuidSchema.nullable(),
  prev_event_id: uuidSchema.nullable(),
  next_event_id: uuidSchema.nullable(),
});
