/**
 * Reads about people and their access.
 *
 * `auth`, `team`, `members` and `invitations` are one slice because all four
 * answer the same question from different angles: who is this, who is on the
 * instance, who is on this project, and who has been asked to join.
 *
 * Who is signed in is not here: the session is a store (`shared/api/session`).
 */
import { queryOptions } from '@tanstack/react-query';
import { projectScope } from '@/shared/api/keys';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';

export const userQueries = {
  /** Public SSO configuration, read before anyone signs in. */
  ssoConfig: () =>
    queryOptions({
      queryKey: ['auth', 'sso-config'],
      queryFn: ({ signal }) => orThrow(api().auth.ssoConfig({ signal })),
    }),
  /** The account an SSO login is waiting to link; `not_found` if none. */
  ssoLink: () =>
    queryOptions({
      queryKey: ['auth', 'sso-link'],
      queryFn: ({ signal }) => orThrow(api().auth.ssoLink({ signal })),
    }),
  /** Every invitation read: the list and each token's public view. */
  invitationsRoot: () => ['invitations'] as const,
  /**
   * What an invitation token offers, for the public accept page.
   *
   * `not_found`, `validation` and `gone` (never issued, malformed, expired or
   * already used) all mean "this token buys you nothing", and they are not
   * folded into one verdict here: the page branches on `kind` itself.
   */
  invitation: (token: string) =>
    queryOptions({
      queryKey: [...userQueries.invitationsRoot(), 'token', token],
      queryFn: ({ signal }) =>
        orThrow(api().auth.getInvitation({ path: { token }, signal })),
    }),
  /** Pending and expired invitations. */
  invitations: () =>
    queryOptions({
      queryKey: [...userQueries.invitationsRoot(), 'list'],
      queryFn: ({ signal }) => orThrow(api().invitations.list({ signal })),
    }),
  /**
   * Whether invitation emails can be delivered, and from where. An admin
   * read: the invite form's switch asks this before offering delivery, and
   * a failure degrades to "cannot send" rather than failing the page.
   */
  emailStatus: () =>
    queryOptions({
      queryKey: [...userQueries.invitationsRoot(), 'email-status'],
      queryFn: ({ signal }) =>
        orThrow(api().invitations.emailStatus({ signal })),
    }),
  /** The instance's users with their global role and status. */
  team: () =>
    queryOptions({
      queryKey: ['team'],
      queryFn: ({ signal }) => orThrow(api().team.list({ signal })),
    }),
  /** A project's members with their per-project role. */
  members: (projectId: number) =>
    queryOptions({
      queryKey: [...projectScope(projectId), 'members'],
      queryFn: ({ signal }) =>
        orThrow(
          api().members.list({ path: { project_id: projectId }, signal }),
        ),
    }),
};
