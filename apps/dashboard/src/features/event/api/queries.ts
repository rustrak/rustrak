/**
 * Reads for the event feature.
 */

import type { Event, EventDetail, Result, RustrakError } from '@rustrak/client';
import { Ok } from '@rustrak/client';
import { queryOptions } from '@tanstack/react-query';
import { scope } from '@/shared/api/query-client';
import { createClient } from '@/shared/api/rustrak';

/**
 * Get a single event with full details.
 *
 * @param projectId - The project ID
 * @param issueId - The issue UUID
 * @param eventId - The event UUID
 * @returns The event with full Sentry data
 */

/**
 * Get a single event with full details.
 *
 * @param projectId - The project ID
 * @param issueId - The issue UUID
 * @param eventId - The event UUID
 * @returns The event with full Sentry data
 */
export async function getEventDetail(
  projectId: number,
  issueId: string,
  eventId: string,
): Promise<Result<EventDetail, RustrakError>> {
  const client = await createClient();
  return client.events.get(projectId, issueId, eventId);
}

/**
 * Navigation info for event pagination
 */
export interface EventNavigation {
  currentIndex: number;
  totalCount: number;
  firstEventId: string | null;
  lastEventId: string | null;
  prevEventId: string | null;
  nextEventId: string | null;
}

/**
 * Get the last (most recent) event for an issue.
 *
 * @param projectId - The project ID
 * @param issueId - The issue UUID
 * @returns The last event, or `null` when the issue has no events
 */
export async function getLastEvent(
  projectId: number,
  issueId: string,
): Promise<Result<Event | null, RustrakError>> {
  const client = await createClient();
  // Get events ordered by desc (most recent first), limit to 1
  const response = await client.events.list(projectId, issueId, {
    order: 'desc',
  });

  if (!response.success) {
    return response;
  }

  return Ok(response.data.items[0] ?? null);
}

/**
 * Where an event sits among its issue's events, oldest first. One request:
 * the server counts and finds the neighbours, so the answer is right however
 * many events the issue has.
 */
export async function getEventNavigation(
  projectId: number,
  issueId: string,
  eventId: string,
): Promise<Result<EventNavigation, RustrakError>> {
  const client = await createClient();
  const result = await client.events.navigation(projectId, issueId, eventId);
  if (!result.success) return result;

  const nav = result.data;
  return Ok({
    currentIndex: nav.current_index,
    totalCount: nav.total_count,
    firstEventId: nav.first_event_id,
    lastEventId: nav.last_event_id,
    prevEventId: nav.prev_event_id,
    nextEventId: nav.next_event_id,
  });
}

export const eventQueries = {
  detail: (projectId: number, issueId: string, eventId: string) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'events', issueId, eventId],
      queryFn: () => getEventDetail(projectId, issueId, eventId),
    }),
  navigation: (projectId: number, issueId: string, eventId: string) =>
    queryOptions({
      queryKey: [
        ...scope.project(projectId),
        'events',
        issueId,
        eventId,
        'navigation',
      ],
      queryFn: () => getEventNavigation(projectId, issueId, eventId),
    }),
};
