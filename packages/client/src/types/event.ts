import type { z } from 'zod';
import type {
  eventDetailSchema,
  eventSchema,
  lookupEventsOptionsSchema,
} from '../schemas/event.js';

/** Exact project-scoped user or request lookup, with an optional bound cursor. */
export type LookupEventsOptions = z.infer<typeof lookupEventsOptionsSchema>;

/**
 * Event resource from list endpoint
 */
export type Event = z.infer<typeof eventSchema>;

/**
 * Event detail resource from detail endpoint
 */
export type EventDetail = z.infer<typeof eventDetailSchema>;
