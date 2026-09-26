/**
 * Reads for the project feature.
 *
 * `stats` lives here rather than in a slice of its own: the aggregates are
 * *of a project*, not a concept a user manipulates.
 */

import type {
  EventTimeseries,
  ListProjectsOptions,
  OffsetPaginatedResponse,
  Project,
  ProjectStatsSummary,
  Result,
  RustrakError,
} from '@rustrak/client';
import { queryOptions } from '@tanstack/react-query';
import { scope } from '@/shared/api/query-client';
import { createClient } from '@/shared/api/rustrak';
import { COMMAND_BAR_PROJECT_LIMIT } from '@/shared/config/commands';

/**
 * Get projects with pagination.
 *
 * @param options - Optional pagination options
 * @returns Paginated list of projects, or the failure that stopped it
 */
export async function getProjects(
  options?: ListProjectsOptions,
): Promise<Result<OffsetPaginatedResponse<Project>, RustrakError>> {
  const client = await createClient();
  return client.projects.list(options);
}

/**
 * Get a single project by ID.
 *
 * @param id - Project ID
 * @returns The project
 */
export async function getProject(
  id: number,
): Promise<Result<Project, RustrakError>> {
  const client = await createClient();
  return client.projects.get(id);
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
 */
export async function getProjectEventTimeseries(
  projectId: number,
  period?: string,
  interval?: number,
): Promise<Result<EventTimeseries, RustrakError>> {
  const client = await createClient();
  return client.stats.timeseries(projectId, period, interval);
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
): Promise<Result<ProjectStatsSummary, RustrakError>> {
  const client = await createClient();
  return client.stats.summary(projectId, period);
}

export const projectQueries = {
  list: (options?: ListProjectsOptions) =>
    queryOptions({
      queryKey: [...scope.projects, options],
      queryFn: () => getProjects(options),
    }),
  /**
   * The first page of every project, by name. The command bar, the sidebar's
   * switcher and the new-project form all read this one, so it is fetched
   * once however many of them are on screen.
   */
  all: () => projectQueries.list({ per_page: COMMAND_BAR_PROJECT_LIMIT }),
  detail: (id: number) =>
    queryOptions({
      queryKey: [...scope.project(id), 'detail'],
      queryFn: () => getProject(id),
    }),
  timeseries: (projectId: number, period?: string, interval?: number) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'timeseries', period, interval],
      queryFn: () => getProjectEventTimeseries(projectId, period, interval),
    }),
  summary: (projectId: number, period?: string) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'summary', period],
      queryFn: () => getProjectStatsSummary(projectId, period),
    }),
};
