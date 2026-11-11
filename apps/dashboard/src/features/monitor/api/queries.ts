import type {
  MonitorsListCheckinsData,
  MonitorsStatsData,
  Schedule,
} from '@rustrak/client';
import { queryOptions } from '@tanstack/react-query';
import { projectScope } from '@/shared/api/keys';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';

export const monitorQueries = {
  /**
   * The next runs of a schedule. A read, though it is a POST: the server only
   * parses the schedule. Keyed by value, so going back to a schedule already
   * previewed asks nothing.
   */
  schedulePreview: (body: { schedule: Schedule; timezone: string }) =>
    queryOptions({
      queryKey: ['schedule-preview', body],
      queryFn: ({ signal }) =>
        orThrow(api().schedules.preview({ body, signal })),
    }),
  all: (projectId: number) => [...projectScope(projectId), 'monitors'] as const,
  list: (projectId: number) =>
    queryOptions({
      queryKey: [...monitorQueries.all(projectId), 'list'],
      queryFn: ({ signal }) =>
        orThrow(
          api().monitors.list({ path: { project_id: projectId }, signal }),
        ),
    }),
  stats: (projectId: number, query: MonitorsStatsData['query'] = {}) =>
    queryOptions({
      queryKey: [...monitorQueries.all(projectId), 'stats', query],
      queryFn: ({ signal }) =>
        orThrow(
          api().monitors.stats({
            path: { project_id: projectId },
            query,
            signal,
          }),
        ),
    }),
  /** One monitor: its detail and its check-ins. */
  monitor: (projectId: number, slug: string) =>
    [...monitorQueries.all(projectId), 'monitor', slug] as const,
  detail: (projectId: number, slug: string) =>
    queryOptions({
      queryKey: [...monitorQueries.monitor(projectId, slug), 'detail'],
      queryFn: ({ signal }) =>
        orThrow(
          api().monitors.get({ path: { project_id: projectId, slug }, signal }),
        ),
    }),
  checkIns: (
    projectId: number,
    slug: string,
    query: MonitorsListCheckinsData['query'] = {},
  ) =>
    queryOptions({
      queryKey: [
        ...monitorQueries.monitor(projectId, slug),
        'check-ins',
        query,
      ],
      queryFn: ({ signal }) =>
        orThrow(
          api().monitors.listCheckins({
            path: { project_id: projectId, slug },
            query,
            signal,
          }),
        ),
    }),
};
