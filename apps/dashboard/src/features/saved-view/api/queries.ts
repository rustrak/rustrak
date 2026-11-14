/**
 * Reads for saved views: one page's, as the reader sees them. A project page's
 * live under its project; a page that spans every project (`projectId`
 * undefined) keeps the workspace's.
 */
import { queryOptions } from '@tanstack/react-query';
import { projectScope } from '@/shared/api/keys';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';

export const savedViewQueries = {
  all: (projectId?: number) =>
    projectId === undefined
      ? (['views'] as const)
      : ([...projectScope(projectId), 'views'] as const),
  list: (projectId: number | undefined, page: string) =>
    queryOptions({
      queryKey: [...savedViewQueries.all(projectId), page],
      queryFn: ({ signal }) =>
        orThrow(
          projectId === undefined
            ? api().workspaceViews.list({ query: { page }, signal })
            : api().savedViews.list({
                path: { project_id: projectId },
                query: { page },
                signal,
              }),
        ),
    }),
};
