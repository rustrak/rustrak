/**
 * Writes to the GitHub integration.
 *
 * Creating, linking or unlinking a GitHub issue also writes a line to the
 * issue's activity log, which belongs to another slice: those invalidate the
 * project's whole scope rather than only the GitHub panel.
 */
import type {
  CreateGitHubIssue,
  IssueTemplate,
  UpdateProjectRepository,
} from '@rustrak/client';
import { mutationOptions } from '@tanstack/react-query';
import { projectScope } from '@/shared/api/keys';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';
import { githubQueries } from './queries';

export const githubMutations = {
  /**
   * The form to post to GitHub so it creates an app already configured for
   * this server. Nothing is stored until GitHub calls back.
   */
  manifest: () =>
    mutationOptions({
      mutationFn: (organization: string | undefined) =>
        orThrow(api().github.manifest({ body: { organization } })),
    }),
  /**
   * Removes the app, its installations and every project's repository, so
   * every project's GitHub reads change too.
   */
  disconnect: () =>
    mutationOptions({
      mutationFn: () => orThrow(api().github.disconnect()),
      meta: { invalidates: [githubQueries.root(), ['projects']] },
    }),
  /** What every GitHub issue starts with, before a project's own. */
  setIssueTemplate: () =>
    mutationOptions({
      mutationFn: (template: IssueTemplate) =>
        orThrow(api().github.putIssueTemplate({ body: template })),
      meta: { invalidates: [githubQueries.root()] },
    }),
  setRepository: (projectId: number) =>
    mutationOptions({
      mutationFn: (input: UpdateProjectRepository) =>
        orThrow(
          api().github.putProjectRepository({
            path: { project_id: projectId },
            body: input,
          }),
        ),
      meta: { invalidates: [githubQueries.all(projectId)] },
    }),
  removeRepository: (projectId: number) =>
    mutationOptions({
      mutationFn: () =>
        orThrow(
          api().github.deleteProjectRepository({
            path: { project_id: projectId },
          }),
        ),
      meta: { invalidates: [githubQueries.all(projectId)] },
    }),
  syncCodeowners: (projectId: number) =>
    mutationOptions({
      mutationFn: () =>
        orThrow(
          api().github.syncCodeowners({ path: { project_id: projectId } }),
        ),
      meta: { invalidates: [githubQueries.all(projectId)] },
    }),
  createIssue: (projectId: number, issueId: string) =>
    mutationOptions({
      mutationFn: (input: CreateGitHubIssue) =>
        orThrow(
          api().github.createIssue({
            path: { project_id: projectId, issue_id: issueId },
            body: input,
          }),
        ),
      meta: { invalidates: [projectScope(projectId)] },
    }),
  linkIssue: (projectId: number, issueId: string) =>
    mutationOptions({
      mutationFn: (reference: string) =>
        orThrow(
          api().github.linkIssue({
            path: { project_id: projectId, issue_id: issueId },
            body: { reference },
          }),
        ),
      meta: { invalidates: [projectScope(projectId)] },
    }),
  unlinkIssue: (projectId: number, issueId: string) =>
    mutationOptions({
      mutationFn: (linkId: number) =>
        orThrow(
          api().github.unlinkIssue({
            path: { project_id: projectId, issue_id: issueId, link_id: linkId },
          }),
        ),
      meta: { invalidates: [projectScope(projectId)] },
    }),
  setMyLogin: () =>
    mutationOptions({
      mutationFn: (login: string | null) =>
        orThrow(api().github.putMyLogin({ body: { login } })),
      meta: { invalidates: [githubQueries.myLogin().queryKey] },
    }),
};
