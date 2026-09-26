import type { z } from 'zod';
import type {
  eventDetailSchema,
  eventNavigationSchema,
  eventSchema,
} from '../schemas/event.js';

/**
 * Event resource from list endpoint
 */
export type Event = z.infer<typeof eventSchema>;

/**
 * Event detail resource from detail endpoint
 */
export type EventDetail = z.infer<typeof eventDetailSchema>;

/**
 * An event's position and neighbours within its issue
 */
export type EventNavigation = z.infer<typeof eventNavigationSchema>;
