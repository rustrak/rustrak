import { z } from 'zod';
import { dateTimeSchema, uuidSchema } from './common.js';

/**
 * Log response schema from the list endpoint (Sentry "log" item — OurLog).
 *
 * `attributes` keeps the OTel-style typed map as received
 * (`{"key":{"value":x,"type":"string"}}`); denormalized fields are surfaced
 * separately for filtering/sorting.
 */
export const logSchema = z.object({
  id: uuidSchema,
  environment: z.string().nullable(),
  trace_id: z.string().nullable(),
  span_id: z.string().nullable(),
  level: z.string(),
  /** OTel severity number (1=trace … 21=fatal). Null for unknown levels. */
  severity_number: z.number().int().nullable(),
  body: z.string(),
  attributes: z.record(z.string(), z.any()),
  timestamp: dateTimeSchema,
  ingested_at: dateTimeSchema,
});
