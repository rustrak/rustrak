/**
 * Reads for the GitHub integration: the instance's connection and the
 * reader's login under `['github']`, a project's repository and what an issue
 * knows from GitHub under the project's scope.
 */
import { queryOptions } from '@tanstack/react-query';
import { projectScope } from '@/shared/api/keys';
import { ApiError, orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';

export const githubQueries = {
  /** Every instance-wide GitHub read. */
  root: () => ['github'] as const,
  /** Every GitHub read of one project. */
  all: (projectId: number) => [...projectScope(projectId), 'github'] as const,
  /**
   * The app and its installations, or `null` for a member: the connection is
   * an admin's, and the server's 403 is that answer, not a failure.
   */
  status: () =>
    queryOptions({
      queryKey: [...githubQueries.root(), 'status'],
      queryFn: async ({ signal }) => {
        const result = await api().github.status({ signal });
        if (result.success) return result.data;
        if (result.error.kind === 'forbidden') return null;
        throw new ApiError(result.error);
      },
    }),
  /** The reader's own GitHub login, which maps GitHub users to them. */
  myLogin: () =>
    queryOptions({
      queryKey: [...githubQueries.root(), 'me'],
      queryFn: ({ signal }) => orThrow(api().github.getMyLogin({ signal })),
    }),
  /** The repositories the installation can see, to pick one for a project. */
  repositories: (projectId: number) =>
    queryOptions({
      queryKey: [...githubQueries.all(projectId), 'repositories'],
      queryFn: ({ signal }) =>
        orThrow(
          api().github.repositories({
            query: { project_id: projectId },
            signal,
          }),
        ),
    }),
  /** The project's repository, or `null` when none is set. */
  repository: (projectId: number) =>
    queryOptions({
      queryKey: [...githubQueries.all(projectId), 'repository'],
      queryFn: ({ signal }) =>
        orThrow(
          api().github.getProjectRepository({
            path: { project_id: projectId },
            signal,
          }),
        ),
    }),
  /** What creating a GitHub issue sends, templates applied, to edit first. */
  draft: (projectId: number, issueId: string) =>
    queryOptions({
      queryKey: [...githubQueries.all(projectId), 'draft', issueId],
      queryFn: ({ signal }) =>
        orThrow(
          api().github.issueDraft({
            path: { project_id: projectId, issue_id: issueId },
            signal,
          }),
        ),
      // Read when the dialog opens, never kept: the latest event changes.
      gcTime: 0,
    }),
  issue: (projectId: number, issueId: string) =>
    queryOptions({
      queryKey: [...githubQueries.all(projectId), 'issue', issueId],
      queryFn: ({ signal }) =>
        orThrow(
          api().github.issue({
            path: { project_id: projectId, issue_id: issueId },
            signal,
          }),
        ),
    }),
};
