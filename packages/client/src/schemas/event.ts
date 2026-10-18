import { z } from 'zod';
import { dateTimeSchema, eventIdSchema, uuidSchema } from './common.js';

const lookupIdentifierSchema = z
  .string()
  .min(1)
  .refine(
    (value) =>
      new TextEncoder().encode(value).length <= 200 &&
      value.trim() === value &&
      !Array.from(value).some((character) => {
        const code = character.charCodeAt(0);
        return code <= 31 || (code >= 127 && code <= 159);
      }),
    'Lookup identifier must contain 1..200 UTF-8 bytes without control or surrounding whitespace',
  );
const lookupCursorSchema = z.string().min(1).max(2048).optional();

/** Exactly one identity selector, with an optional continuation from that search. */
export const lookupEventsOptionsSchema = z.union([
  z
    .object({ user_id: lookupIdentifierSchema, cursor: lookupCursorSchema })
    .strict(),
  z
    .object({ request_id: lookupIdentifierSchema, cursor: lookupCursorSchema })
    .strict(),
]);

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
