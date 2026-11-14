/**
 * Writes to issues.
 *
 * A change of status, a delete or a bulk action moves the project's counts
 * (the overview, the stats, the release pages), which live outside this slice:
 * those invalidate the whole `projectScope`, and a batch that may span
 * projects every project's.
 */
import type {
  BulkUpdateIssues,
  Result,
  RustrakError,
  UpdateIssueState,
} from '@rustrak/client';
import { mutationOptions } from '@tanstack/react-query';
import { byProject } from '@/features/issue/model/issue-table';
import { allProjects, projectScope } from '@/shared/api/keys';
import { ApiError, orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';
import { issueQueries } from './queries';

/** An issue as a write needs it: which one, and whose. */
type IssueRef = { id: string; project_id: number };

/**
 * Waits for every request, then throws the first failure: the others went
 * through, and the refetch has to show them.
 */
async function settled(requests: Promise<Result<unknown, RustrakError>>[]) {
  for (const result of await Promise.all(requests)) {
    if (!result.success) throw new ApiError(result.error);
  }
}

export const issueMutations = {
  /**
   * Resolve, mute, reprioritise… `{ status: 'resolvedInNextRelease' }` marks
   * the issue resolved but tracks the pending release, so a regression before
   * the deploy reopens it and a recorded deploy finalises it.
   *
   * The issue is a variable, not a parameter: a list acts on whichever row
   * the reader is on.
   */
  update: (projectId: number) =>
    mutationOptions({
      mutationFn: ({
        issueId,
        state,
      }: {
        issueId: string;
        state: UpdateIssueState;
      }) =>
        orThrow(
          api().issues.update({
            path: { project_id: projectId, issue_id: issueId },
            body: state,
          }),
        ),
      meta: {
        invalidates: [projectScope(projectId), issueQueries.everyProject()],
      },
    }),
  /**
   * Assigns some issues. The API assigns one issue at a time, so this is one
   * request per issue, all of them settled before the first failure is
   * thrown: the others went through, and the refetch has to show them.
   */
  assign: () =>
    mutationOptions({
      mutationFn: async ({
        issues,
        assignedTo,
      }: {
        issues: IssueRef[];
        assignedTo: number | null;
      }) =>
        settled(
          issues.map(({ id, project_id }) =>
            api().issues.update({
              path: { project_id, issue_id: id },
              body: { assigned_to: assignedTo },
            }),
          ),
        ),
      meta: { invalidates: [allProjects()] },
    }),
  /**
   * Sets the status or priority of some issues, of one project or several:
   * one request per project, all settled before the first failure is thrown.
   */
  bulkUpdate: () =>
    mutationOptions({
      mutationFn: ({
        issues,
        ...change
      }: Omit<BulkUpdateIssues, 'ids'> & { issues: IssueRef[] }) =>
        settled(
          [...byProject(issues)].map(([project_id, ids]) =>
            api().issues.bulkUpdate({
              path: { project_id },
              body: { ids, ...change },
            }),
          ),
        ),
      meta: { invalidates: [allProjects()] },
    }),
  /** Deletes some issues, one request per project, as `bulkUpdate`. */
  bulkDelete: () =>
    mutationOptions({
      mutationFn: (issues: IssueRef[]) =>
        settled(
          [...byProject(issues)].map(([project_id, ids]) =>
            api().issues.bulkDelete({ path: { project_id }, body: { ids } }),
          ),
        ),
      // Dropped before the lists refetch, as `delete`: asking again for a
      // deleted issue can only answer 404.
      onSuccess: (_data, issues, _result, { client }) => {
        for (const { id, project_id } of issues) {
          client.removeQueries({ queryKey: issueQueries.one(project_id, id) });
        }
      },
      meta: { invalidates: [allProjects()] },
    }),
  delete: (projectId: number) =>
    mutationOptions({
      mutationFn: (issueId: string) =>
        orThrow(
          api().issues.delete({
            path: { project_id: projectId, issue_id: issueId },
          }),
        ),
      // Dropped before the project refetches: asking again for what was
      // just deleted can only answer 404.
      onSuccess: (_data, issueId, _result, { client }) =>
        client.removeQueries({
          queryKey: issueQueries.one(projectId, issueId),
        }),
      meta: {
        invalidates: [projectScope(projectId), issueQueries.everyProject()],
      },
    }),
  /** A note on the issue's activity log. */
  comment: (projectId: number, issueId: string) =>
    mutationOptions({
      mutationFn: (text: string) =>
        orThrow(
          api().issues.createComment({
            path: { project_id: projectId, issue_id: issueId },
            body: { text },
          }),
        ),
      meta: { invalidates: [issueQueries.one(projectId, issueId)] },
    }),
  /** The reader's own bookmark, which the lists can filter on. */
  bookmark: (projectId: number, issueId: string) =>
    mutationOptions({
      mutationFn: (enabled: boolean) =>
        orThrow(
          api().issues.setBookmark({
            path: { project_id: projectId, issue_id: issueId },
            body: { enabled },
          }),
        ),
      meta: {
        invalidates: [
          issueQueries.one(projectId, issueId),
          issueQueries.lists(projectId),
        ],
      },
    }),
  /** The reader's own subscription to the issue's notifications. */
  subscribe: (projectId: number, issueId: string) =>
    mutationOptions({
      mutationFn: (enabled: boolean) =>
        orThrow(
          api().issues.setSubscription({
            path: { project_id: projectId, issue_id: issueId },
            body: { enabled },
          }),
        ),
      meta: { invalidates: [issueQueries.one(projectId, issueId)] },
    }),
};
