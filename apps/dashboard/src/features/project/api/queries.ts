/**
 * Reads for the project feature.
 *
 * `stats` lives here rather than in a slice of its own: the aggregates are
 * *of a project*, not a concept a user manipulates.
 */
import type {
  ProjectResponse,
  ProjectStatsSummary,
  ProjectsListData,
  ProjectsListResponse,
  RateLimitsResponse,
  Result,
  RustrakError,
  StatsGetEventTimeseriesResponse,
} from '@rustrak/client';
import { mapResult } from '@rustrak/client';
import { createClient } from '@/shared/api/rustrak';
import { shareInFlight } from '@/shared/lib/in-flight';

/** What the overview's page filters narrow every read to. */
export interface OverviewScope {
  environment?: string;
  release?: string;
}

/**
 * Get projects with pagination.
 *
 * @param options - Optional pagination options
 * @returns Paginated list of projects, or the failure that stopped it
 */
export async function getProjects(
  options?: ProjectsListData['query'],
): Promise<Result<ProjectsListResponse, RustrakError>> {
  const client = await createClient();
  return client.projects.list({ query: options });
}

/**
 * Get a single project by ID.
 *
 * @param id - Project ID
 * @returns The project
 */
export function getProject(
  id: number,
): Promise<Result<ProjectResponse, RustrakError>> {
  return shareInFlight(`project:${id}`, async () => {
    const client = await createClient();
    return client.projects.get({ path: { id } });
  });
}

/** The server's per-project limits, which a project without its own follows. */
export async function getRateLimits(): Promise<
  Result<RateLimitsResponse, RustrakError>
> {
  const client = await createClient();
  return client.projects.getRateLimits();
}

/**
 * Get time-bucketed error-event volume for a project, split by severity.
 *
 * No catch: a fetch/auth failure must surface to the error boundary rather
 * than be disguised as a flat line at zero, which reads as "nothing is
 * breaking" — the most dangerous thing this page could say while wrong.
 *
 * @param projectId - The project ID
 * @param period - Time window (e.g. '24h', '7d'). Omit for all time.
 * @param interval - Bucket width in hours (default: 1, max: 24).
 * @param scope - Only this environment and release. Omit for every one.
 */
export async function getProjectEventTimeseries(
  projectId: number,
  period?: string,
  interval?: number,
  scope?: OverviewScope,
): Promise<Result<StatsGetEventTimeseriesResponse, RustrakError>> {
  const client = await createClient();
  return client.stats.getEventTimeseries({
    path: { project_id: projectId },
    query: { period, interval, ...scope },
  });
}

/**
 * Get project-wide counters for the window, each with its previous-period
 * comparison.
 *
 * No catch, for the same reason as the timeseries above: zeroed counters are
 * indistinguishable from a genuinely quiet project.
 *
 * @param projectId - The project ID
 * @param period - Time window (e.g. '24h', '7d'). Omit for all time.
 */
export async function getProjectStatsSummary(
  projectId: number,
  period?: string,
  scope?: OverviewScope,
): Promise<Result<ProjectStatsSummary, RustrakError>> {
  const client = await createClient();
  return client.stats.getSummary({
    path: { project_id: projectId },
    query: { period, ...scope },
  });
}

/** The newest hundred releases' versions, for the filters that pick one. */
export async function getProjectReleases(
  projectId: number,
): Promise<Result<string[], RustrakError>> {
  const client = await createClient();
  const page = await client.releases.list({
    path: { project_id: projectId },
    query: { per_page: 100 },
  });
  return mapResult(page, (p) => p.items.map((release) => release.version));
}

/** The environments a project's issues have been seen in, by name. */
export async function getProjectEnvironments(
  projectId: number,
): Promise<Result<string[], RustrakError>> {
  const client = await createClient();
  return client.environments.list({ path: { project_id: projectId } });
}
