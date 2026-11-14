/**
 * Reads for the project feature.
 *
 * `stats` lives here rather than in a slice of its own: the aggregates are
 * *of a project*, not a concept a user manipulates.
 */
import type { ProjectsListData } from '@rustrak/client';
import { queryOptions } from '@tanstack/react-query';
import { allProjects, projectScope } from '@/shared/api/keys';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';

/**
 * How many projects a picker (the sidebar, the command bar, the token and team
 * dialogs) carries. The frame reads it on *every* authenticated page, so it is
 * one request, not a page-through; an instance past this many needs a
 * server-side search, which is a different change to making this bigger.
 */
const PICKER_LIMIT = 100;

interface OverviewScope {
  environment?: string;
  release?: string;
}

export const projectQueries = {
  lists: () => [...allProjects(), 'list'] as const,
  list: (query: ProjectsListData['query'] = {}) =>
    queryOptions({
      queryKey: [...projectQueries.lists(), query],
      queryFn: ({ signal }) => orThrow(api().projects.list({ query, signal })),
    }),
  /** The first projects, for every picker; one cache entry for all of them. */
  picker: () => projectQueries.list({ per_page: PICKER_LIMIT }),
  detail: (id: number) =>
    queryOptions({
      queryKey: [...projectScope(id), 'detail'],
      queryFn: ({ signal }) =>
        orThrow(api().projects.get({ path: { id }, signal })),
    }),
  /** The server's per-project limits, which a project without its own follows. */
  rateLimits: () =>
    queryOptions({
      queryKey: ['rate-limits'],
      queryFn: ({ signal }) =>
        orThrow(api().projects.getRateLimits({ signal })),
    }),
  eventTimeseries: (
    projectId: number,
    query: { period?: string; interval?: number } & OverviewScope,
  ) =>
    queryOptions({
      queryKey: [...projectScope(projectId), 'stats', 'events', query],
      queryFn: ({ signal }) =>
        orThrow(
          api().stats.getEventTimeseries({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
  summary: (projectId: number, query: { period?: string } & OverviewScope) =>
    queryOptions({
      queryKey: [...projectScope(projectId), 'stats', 'summary', query],
      queryFn: ({ signal }) =>
        orThrow(
          api().stats.getSummary({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
  /** The newest hundred releases' versions, for the filters that pick one. */
  releaseVersions: (projectId: number) =>
    queryOptions({
      queryKey: [...projectScope(projectId), 'releases', 'versions'],
      queryFn: async ({ signal }) => {
        const page = await orThrow(
          api().releases.list({
            path: { project_id: projectId },
            query: { per_page: 100 },
            signal,
          }),
        );
        return page.items.map((release) => release.version);
      },
    }),
  /**
   * The environments a project's issues have been seen in, by name.
   *
   * No `signal`, unlike every other read: most pages filter by environment,
   * so the page being left and the page being opened both observe this. A
   * read that consumed its signal is cancelled when its last observer goes,
   * and the old page unmounts after the new one's loader has started it, so
   * every navigation between two such pages asked twice. A short list is not
   * worth cancelling.
   */
  environments: (projectId: number) =>
    queryOptions({
      queryKey: [...projectScope(projectId), 'environments'],
      queryFn: () =>
        orThrow(api().environments.list({ path: { project_id: projectId } })),
    }),
};
