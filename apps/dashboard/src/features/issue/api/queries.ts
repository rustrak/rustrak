/**
 * Reads for the issues feature.
 */

import type {
  ActivityEntry,
  Issue,
  IssueAggregates,
  IssueStats,
  IssueStatsWindow,
  ListIssuesOptions,
  OffsetPaginatedResponse,
  Result,
  RustrakError,
} from '@rustrak/client';
import { queryOptions } from '@tanstack/react-query';
import { invalidate, scope } from '@/shared/api/query-client';
import { createClient } from '@/shared/api/rustrak';

/**
 * List issues for a project with offset-based pagination.
 *
 * @param projectId - The project ID
 * @param options - Optional filtering and pagination options
 * @returns Paginated list of issues with total count
 */

/**
 * List issues for a project with offset-based pagination.
 *
 * @param projectId - The project ID
 * @param options - Optional filtering and pagination options
 * @returns Paginated list of issues with total count
 */
export async function listIssues(
  projectId: number,
  options?: ListIssuesOptions,
): Promise<Result<OffsetPaginatedResponse<Issue>, RustrakError>> {
  const client = await createClient();
  return client.issues.list(projectId, options);
}

/**
 * Get a single issue by ID.
 *
 * @param projectId - The project ID
 * @param issueId - The issue UUID
 * @returns The issue
 */
export async function getIssue(
  projectId: number,
  issueId: string,
): Promise<Result<Issue, RustrakError>> {
  const client = await createClient();
  return client.issues.get(projectId, issueId);
}

/**
 * Get per-issue aggregates (unique user count + top tags).
 *
 * @param projectId - The project ID
 * @param issueId - The issue UUID
 */
export async function getIssueAggregates(
  projectId: number,
  issueId: string,
): Promise<Result<IssueAggregates, RustrakError>> {
  const client = await createClient();
  return client.issues.getAggregates(projectId, issueId);
}

/**
 * Get a zero-filled event-count timeseries for an issue (24h or 30d).
 *
 * @param projectId - The project ID
 * @param issueId - The issue UUID
 * @param window - The time window (`24h` or `30d`)
 */
export async function getIssueStats(
  projectId: number,
  issueId: string,
  window: IssueStatsWindow = '24h',
): Promise<Result<IssueStats, RustrakError>> {
  const client = await createClient();
  return client.issues.getStats(projectId, issueId, window);
}

/**
 * Get an issue's activity log (status changes, comments/notes, etc.).
 *
 * @param projectId - The project ID
 * @param issueId - The issue UUID
 */
export async function getIssueActivity(
  projectId: number,
  issueId: string,
): Promise<Result<ActivityEntry[], RustrakError>> {
  const client = await createClient();
  return client.issues.getActivity(projectId, issueId);
}

export const issueQueries = {
  list: (projectId: number, options?: ListIssuesOptions) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'issues', 'list', options],
      queryFn: () => listIssues(projectId, options),
    }),
  detail: (projectId: number, issueId: string) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'issues', issueId],
      queryFn: () => getIssue(projectId, issueId),
    }),
  aggregates: (projectId: number, issueId: string) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'issues', issueId, 'aggregates'],
      queryFn: () => getIssueAggregates(projectId, issueId),
    }),
  stats: (projectId: number, issueId: string, window: IssueStatsWindow) =>
    queryOptions({
      queryKey: [
        ...scope.project(projectId),
        'issues',
        issueId,
        'stats',
        window,
      ],
      queryFn: () => getIssueStats(projectId, issueId, window),
    }),
  activity: (projectId: number, issueId: string) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'issues', issueId, 'activity'],
      queryFn: () => getIssueActivity(projectId, issueId),
    }),
};

/**
 * After a write to issues: the issue reads and the counts built on them. The
 * event on screen and its prev/next navigation cannot have changed, and the
 * navigation is the most expensive read on the page.
 */
export function invalidateIssues(projectId: number): Promise<void> {
  return Promise.all([
    invalidate([...scope.project(projectId), 'issues']),
    invalidate([...scope.project(projectId), 'summary']),
    invalidate(scope.projects),
  ]).then(() => undefined);
}
