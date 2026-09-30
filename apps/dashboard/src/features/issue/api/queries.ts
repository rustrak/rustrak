/**
 * Reads for the issues feature, called straight from Server Components.
 *
 * `import 'server-only'` is a build-time poison pill rather than a directive:
 * if this module ever reaches the client bundle the build fails, instead of
 * shipping a browser bundle that holds the session cookie.
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
  environment?: string,
): Promise<Result<Issue, RustrakError>> {
  const client = await createClient();
  return client.issues.get(projectId, issueId, environment);
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
  environment?: string,
): Promise<Result<IssueAggregates, RustrakError>> {
  const client = await createClient();
  return client.issues.getAggregates(projectId, issueId, environment);
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
  environment?: string,
): Promise<Result<IssueStats, RustrakError>> {
  const client = await createClient();
  return client.issues.getStats(projectId, issueId, window, environment);
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
