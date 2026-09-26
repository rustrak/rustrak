/**
 * Reads for the release feature.
 *
 * Releases and sessions are one slice because "release health" *is* sessions
 * grouped by release: the two were never separate concepts, only separate
 * endpoints.
 */

import type {
  Issue,
  OffsetPaginatedResponse,
  ReleaseHealthRow,
  ReleaseHealthStatsOptions,
  Result,
  RustrakError,
  SessionSummary,
  SessionTimeseries,
} from '@rustrak/client';
import { Ok } from '@rustrak/client';
import { queryOptions } from '@tanstack/react-query';
import { scope } from '@/shared/api/query-client';
import { createClient } from '@/shared/api/rustrak';

/**
 * Get issues first seen in a given release, most recently introduced first.
 *
 * The failure is returned, not swallowed. An empty array here renders "no new
 * issues introduced in this release", which is a claim about the release; a
 * fetch that failed makes no claim at all, and the two must not look the same
 * on a product whose whole job is telling you what broke.
 *
 * @param projectId - The project ID
 * @param release - Release version
 * @param limit - Max issues to return (default: 10, max: 50)
 */
export async function getNewIssuesForRelease(
  projectId: number,
  release: string,
  limit?: number,
): Promise<Result<Issue[], RustrakError>> {
  const client = await createClient();
  return client.releases.newIssues(projectId, release, limit);
}

/**
 * Get one page of per-release health stats for a project.
 *
 * The failure is returned, not swallowed: an empty page and an unreachable API
 * must not render the same way.
 *
 * @param projectId - The project ID
 * @param options - Time window, release scoping and pagination.
 */
export async function getReleaseHealth(
  projectId: number,
  options?: ReleaseHealthStatsOptions,
): Promise<Result<OffsetPaginatedResponse<ReleaseHealthRow>, RustrakError>> {
  const client = await createClient();
  return client.sessions.stats(projectId, options);
}

/** Page size used when walking every row of a single release. */
const RELEASE_ROWS_PER_PAGE = 100;

// Backstop on a bound the server supplies. At 100 rows a page this still
// allows 2000 release/environment rows, which is far past any real project.
const MAX_RELEASE_PAGES = 20;

/**
 * Get every health row for one release, across all its environments.
 *
 * The release detail page needs the complete set: a row missing from the
 * response renders an environment's cards blank even though the list linked
 * to it. One page holds every realistic environment count, so the loop
 * normally runs once, but it keeps going when a project has more.
 *
 * A page that fails ends the call as a failure. Returning the rows gathered so
 * far would be the same silent truncation the complete set exists to avoid.
 *
 * `total_pages` comes from the response body, so the loop is bounded by
 * something the server controls. `MAX_RELEASE_PAGES` is the backstop: a server
 * bug reporting an inflated count would otherwise hold a Server Component
 * render open for as many sequential round-trips as it cared to name.
 *
 * @param projectId - The project ID
 * @param release - The release version to scope to
 * @param period - Time window (e.g. '24h', '7d'). Omit for all time.
 */
export async function getAllReleaseHealthRows(
  projectId: number,
  release: string,
  period?: string,
): Promise<Result<ReleaseHealthRow[], RustrakError>> {
  const client = await createClient();

  const rows: ReleaseHealthRow[] = [];
  let page = 1;
  let totalPages = 1;

  do {
    const response = await client.sessions.stats(projectId, {
      release,
      period,
      page,
      per_page: RELEASE_ROWS_PER_PAGE,
    });
    if (!response.success) {
      return response;
    }
    rows.push(...response.data.items);
    totalPages = Math.min(response.data.total_pages, MAX_RELEASE_PAGES);
    page += 1;
  } while (page <= totalPages);

  return Ok(rows);
}

/**
 * Get project-wide session health, aggregated across all releases and environments.
 *
 * The failure is returned, not swallowed. This used to fall back to a zeroed
 * `SessionSummary`, which is the one fallback shape that cannot be spotted by
 * reading a render: zero sessions, zero crashed, zero abnormal is a real window
 * a healthy project can have, so an outage rendered as a confident "nothing is
 * wrong". Nothing in the type system, the compiler or `next build` can see the
 * difference, which is exactly why the difference has to live in the `Result`.
 *
 * @param projectId - The project ID
 * @param period - Time window (e.g. '24h', '7d'). Defaults to '24h'.
 */
export async function getSessionSummary(
  projectId: number,
  period?: string,
): Promise<Result<SessionSummary, RustrakError>> {
  const client = await createClient();
  return client.sessions.summary(projectId, period);
}

/**
 * Get a time-bucketed session trend for a project, aggregated across all
 * releases and environments.
 *
 * Returns the failure for the same reason as {@link getSessionSummary}: an
 * empty series draws a chart that says "no sessions in this window", which an
 * unreachable API is in no position to say.
 *
 * @param projectId - The project ID
 * @param period - Time window (e.g. '24h', '7d'). Defaults to '24h'.
 * @param interval - Bucket width in hours (default: 1, max: 24).
 */
export async function getSessionTimeseries(
  projectId: number,
  period?: string,
  interval?: number,
): Promise<Result<SessionTimeseries, RustrakError>> {
  const client = await createClient();
  return client.sessions.timeseries(projectId, period, interval);
}

export const releaseQueries = {
  newIssues: (projectId: number, release: string, limit?: number) =>
    queryOptions({
      queryKey: [
        ...scope.project(projectId),
        'releases',
        release,
        'new-issues',
        limit,
      ],
      queryFn: () => getNewIssuesForRelease(projectId, release, limit),
    }),
  health: (projectId: number, options?: ReleaseHealthStatsOptions) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'releases', 'health', options],
      queryFn: () => getReleaseHealth(projectId, options),
    }),
  rows: (projectId: number, release: string) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'releases', release, 'rows'],
      queryFn: () => getAllReleaseHealthRows(projectId, release),
    }),
  sessionSummary: (projectId: number, period?: string) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'sessions', 'summary', period],
      queryFn: () => getSessionSummary(projectId, period),
    }),
  sessionTimeseries: (projectId: number, period?: string, interval?: number) =>
    queryOptions({
      queryKey: [
        ...scope.project(projectId),
        'sessions',
        'timeseries',
        period,
        interval,
      ],
      queryFn: () => getSessionTimeseries(projectId, period, interval),
    }),
};
