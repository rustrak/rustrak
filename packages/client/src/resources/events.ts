import type { RustrakError } from '../errors.js';
import type { Result } from '../result.js';
import {
  eventDetailSchema,
  eventNavigationSchema,
  eventSchema,
  paginatedResponseSchema,
} from '../schemas/index.js';
import type {
  Event,
  EventDetail,
  EventNavigation,
  ListEventsOptions,
  PaginatedResponse,
} from '../types/index.js';
import { BaseResource } from './base.js';

/**
 * Events API resource
 */
export class EventsResource extends BaseResource {
  /**
   * List events for an issue with pagination
   */
  async list(
    projectId: number,
    issueId: string,
    options?: ListEventsOptions,
  ): Promise<Result<PaginatedResponse<Event>, RustrakError>> {
    const searchParams: Record<string, string> = {};

    if (options?.order) {
      searchParams.order = options.order;
    }
    if (options?.cursor) {
      searchParams.cursor = options.cursor;
    }

    return this.request(
      () =>
        this.http.get(`api/projects/${projectId}/issues/${issueId}/events`, {
          searchParams,
        }),
      paginatedResponseSchema(eventSchema),
    );
  }

  /**
   * Get a single event by ID with full details
   */
  async get(
    projectId: number,
    issueId: string,
    eventId: string,
  ): Promise<Result<EventDetail, RustrakError>> {
    return this.request(
      () =>
        this.http.get(
          `api/projects/${projectId}/issues/${issueId}/events/${eventId}`,
        ),
      eventDetailSchema,
    );
  }

  /**
   * An event's position among its issue's events, oldest first, and the ids
   * of its first, last, previous and next siblings
   */
  async navigation(
    projectId: number,
    issueId: string,
    eventId: string,
  ): Promise<Result<EventNavigation, RustrakError>> {
    return this.request(
      () =>
        this.http.get(
          `api/projects/${projectId}/issues/${issueId}/events/${eventId}/navigation`,
        ),
      eventNavigationSchema,
    );
  }
}
