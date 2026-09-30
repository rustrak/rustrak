/**
 * Reads for the event feature, called straight from Server Components.
 *
 * `import 'server-only'` is a build-time poison pill rather than a directive:
 * if this module reaches the client bundle the build fails, instead of shipping
 * a browser bundle that holds the session cookie.
 */
import type { Event, EventDetail, Result, RustrakError } from '@rustrak/client';
import { Ok } from '@rustrak/client';
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
  environment?: string,
): Promise<Result<Event | null, RustrakError>> {
  const client = await createClient();
  // Get events ordered by desc (most recent first), limit to 1
  const response = await client.events.list(projectId, issueId, {
    order: 'desc',
    environment,
  });

  if (!response.success) {
    return response;
  }

  return Ok(response.data.items[0] ?? null);
}

// Caps sequential page fetches in getEventNavigation to bound worst-case
// round-trips for high-volume issues (~1000 events at PAGE_SIZE=20).
const MAX_NAV_PAGES = 50;

/**
 * Get navigation info for an event within an issue.
 * This fetches events (up to MAX_NAV_PAGES pages) to determine prev/next,
 * which works for issues with reasonable event counts. For very large
 * issues, navigation is best-effort within the fetched window.
 *
 * A page that *fails* ends the call as a failure rather than truncating the
 * window, because nothing downstream could tell a truncated window apart from
 * a genuinely short issue.
 *
 * A page that merely *runs out of budget* still truncates, and that path is a
 * known defect rather than a design: on hitting `MAX_NAV_PAGES` this returns
 * `Ok` with a partial `totalCount` and no signal. Because the issue page lands
 * the reader on the newest event while this walks ascending, the current event
 * usually falls outside a truncated window, `findIndex` returns `-1`, and the
 * result reads "0 of N" with "next" pointing at the oldest event in the issue.
 * Pre-existing, unchanged by the `Result` conversion, and recorded as D-31 in
 * deferred-work.md; the fix is a different navigation design (ask the server
 * for neighbours) rather than a bigger cap, which only moves the threshold.
 *
 * @param projectId - The project ID
 * @param issueId - The issue UUID
 * @param currentEventId - The current event UUID
 * @returns Navigation info with prev/next event IDs
 */
export async function getEventNavigation(
  projectId: number,
  issueId: string,
  currentEventId: string,
  environment?: string,
): Promise<Result<EventNavigation, RustrakError>> {
  const client = await createClient();

  // Fetch events in ascending order (oldest first), following the cursor
  // chain, so navigation is correct for issues with more than one page.
  const events: Event[] = [];
  let cursor: string | undefined;
  let pageCount = 0;
  do {
    const response = await client.events.list(projectId, issueId, {
      order: 'asc',
      cursor,
      environment,
    });
    if (!response.success) {
      return response;
    }
    events.push(...response.data.items);
    cursor = response.data.has_more ? response.data.next_cursor : undefined;
    pageCount++;
  } while (cursor && pageCount < MAX_NAV_PAGES);

  const totalCount = events.length;

  if (totalCount === 0) {
    return Ok({
      currentIndex: 0,
      totalCount: 0,
      firstEventId: null,
      lastEventId: null,
      prevEventId: null,
      nextEventId: null,
    });
  }

  // Find current event index
  const currentIndex = events.findIndex((event) => event.id === currentEventId);

  return Ok({
    currentIndex: currentIndex + 1, // 1-based for display
    totalCount,
    firstEventId: events[0]?.id ?? null,
    lastEventId: events[totalCount - 1]?.id ?? null,
    prevEventId:
      currentIndex > 0 ? (events[currentIndex - 1]?.id ?? null) : null,
    nextEventId:
      currentIndex < totalCount - 1
        ? (events[currentIndex + 1]?.id ?? null)
        : null,
  });
}
