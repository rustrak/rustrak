import type { RustrakError } from '../errors.js';
import type { Result } from '../result.js';
import {
  eventDetailSchema,
  eventSchema,
  lookupEventsOptionsSchema,
  paginatedResponseSchema,
} from '../schemas/index.js';
import type {
  Event,
  EventDetail,
  ListEventsOptions,
  LookupEventsOptions,
  PaginatedResponse,
} from '../types/index.js';
import { BaseResource } from './base.js';

/**
 * Events API resource
 */
export class EventsResource extends BaseResource {
  /** Look up exact user.id or tags["request.id"] matches across a project's issues. */
  async lookup(
    projectId: number,
    options: LookupEventsOptions,
  ): Promise<Result<PaginatedResponse<Event>, RustrakError>> {
    const validated = this.validateInput(options, lookupEventsOptionsSchema);
    if (!validated.success) return validated;
    const searchParams: Record<string, string> =
      'user_id' in validated.data
        ? { user_id: validated.data.user_id }
        : { request_id: validated.data.request_id };
    if (validated.data.cursor) searchParams.cursor = validated.data.cursor;
    return this.request(
      () =>
        this.http.get(`api/projects/${projectId}/events/lookup`, {
          searchParams,
        }),
      paginatedResponseSchema(eventSchema),
    );
  }

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

  /** Resolve the client-supplied Sentry event ID, including its owning issue. */
  async getBySentryId(
    projectId: number,
    eventId: string,
  ): Promise<Result<EventDetail, RustrakError>> {
    return this.request(
      () =>
        this.http.get(
          `api/projects/${projectId}/events/sentry/${encodeURIComponent(eventId)}`,
        ),
      eventDetailSchema,
    );
  }
}
