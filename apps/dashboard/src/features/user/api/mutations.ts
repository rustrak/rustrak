/**
 * Writes about people and their access.
 *
 * The sign-in writes publish the user they are handed to the session store
 * themselves: the response already carries it, and a second `/auth/me` to
 * learn what we were just told is a round trip the login screen pays for.
 * No cookie handling anywhere: the request is same-origin, so the browser
 * stores the session cookie itself.
 */
import type {
  AcceptInvitation,
  CreateInvitation,
  LoginRequest,
  UpsertProjectMember,
  UserRole,
} from '@rustrak/client';
import { mutationOptions } from '@tanstack/react-query';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';
import { session } from '@/shared/api/session';
import { userQueries } from './queries';

export const userMutations = {
  /**
   * The server answers the same `unauthenticated` whether the email is
   * unknown, the password wrong or the account disabled; giving that one
   * deliberately vague sentence is the form's decision (`login-form.tsx`).
   */
  login: () =>
    mutationOptions({
      mutationFn: async (credentials: LoginRequest) => {
        const { user } = await orThrow(api().auth.login({ body: credentials }));
        session.set({ state: 'authenticated', user });
        return user;
      },
    }),
  /** Start OIDC login; resolves to the URL to send the browser to. */
  startSso: () =>
    mutationOptions({
      mutationFn: async () =>
        (await orThrow(api().auth.ssoStart())).authorization_url,
    }),
  /** Link the pending SSO identity with the account's password and sign in. */
  confirmSsoLink: () =>
    mutationOptions({
      mutationFn: async (password: string) => {
        const { user } = await orThrow(
          api().auth.confirmSsoLink({ body: { password } }),
        );
        session.set({ state: 'authenticated', user });
        return user;
      },
      // No invalidation of `ssoLink`: the page reading it is on its way out,
      // and refetching the link it just consumed would swap the form for
      // "this link expired" under the reader.
    }),
  /**
   * The session this tab holds is dropped whether or not the request lands,
   * and a failed request is not reported: dropping it is not a claim that the
   * server session ended (the next guard asks and gets the truth), it is a
   * refusal to keep rendering the dashboard, the safer of the two failures on
   * a shared machine.
   */
  logout: () =>
    mutationOptions({
      mutationFn: async () => {
        try {
          await api().auth.logout();
        } finally {
          session.clear();
        }
      },
    }),
  /**
   * Accept an invitation by setting a password. The server signs the new
   * account in unless password login is disabled; then `signed_in` is false,
   * the user signs in through SSO, and the server has also ended any session
   * this browser had, so the store must not keep showing that other account.
   */
  acceptInvitation: () =>
    mutationOptions({
      mutationFn: async (input: AcceptInvitation) => {
        const accepted = await orThrow(
          api().auth.acceptInvitation({ body: input }),
        );
        if (accepted.signed_in) {
          session.set({ state: 'authenticated', user: accepted.user });
        } else {
          session.clear();
        }
        return accepted;
      },
      // The admin's list and roster, not the token's own view: the accept
      // page is standing on it, and the spent token would read as a dead link
      // over the "account created" screen.
      meta: {
        invalidates: [
          userQueries.invitations().queryKey,
          userQueries.team().queryKey,
        ],
      },
    }),
  /**
   * The reader's language and timezone. A field left out is left alone; a
   * field sent as `null` is cleared, back to "infer it from my browser".
   *
   * The updated user goes to the session store, which is not a query and so
   * not something `meta.invalidates` can reach: the row is only read at boot,
   * and without this `intl.reload()` would resolve straight back to the
   * language the reader just changed away from.
   */
  updatePreferences: () =>
    mutationOptions({
      mutationFn: async (preferences: {
        language?: string | null;
        timezone?: string | null;
      }) => {
        const user = await orThrow(
          api().auth.updateCurrentUser({ body: preferences }),
        );
        session.set({ state: 'authenticated', user });
      },
    }),
  /** Replace the signed-in user's password, given the current one. */
  changePassword: () =>
    mutationOptions({
      mutationFn: (request: {
        current_password: string;
        new_password: string;
      }) => orThrow(api().auth.changePassword({ body: request })),
    }),
  /**
   * A user's global role. The server refuses to demote the last admin and
   * names `role` on what it can attribute, so the failure reaches the control.
   * The user is a variable: one mutation serves every row of the roster.
   */
  updateRole: () =>
    mutationOptions({
      mutationFn: ({ userId, role }: { userId: number; role: UserRole }) =>
        orThrow(
          api().team.updateRole({ path: { user_id: userId }, body: { role } }),
        ),
      meta: { invalidates: [userQueries.team().queryKey] },
    }),
  /**
   * Remove a user from the instance; the server refuses yourself, the primary
   * user and the last admin. Their memberships go with them, in every project.
   */
  removeFromTeam: () =>
    mutationOptions({
      mutationFn: (userId: number) =>
        orThrow(api().team.deleteUser({ path: { user_id: userId } })),
      meta: { invalidates: [userQueries.team().queryKey, ['projects']] },
    }),
  /**
   * Add a member to a project or change their role (upsert by `user_id`). The
   * server names `role` on a rejected role.
   */
  upsertMember: (projectId: number) =>
    mutationOptions({
      mutationFn: (input: UpsertProjectMember) =>
        orThrow(
          api().members.upsert({
            path: { project_id: projectId },
            body: input,
          }),
        ),
      meta: { invalidates: [userQueries.members(projectId).queryKey] },
    }),
  /** The member is the variable: one mutation serves every row. */
  removeMember: (projectId: number) =>
    mutationOptions({
      mutationFn: (userId: number) =>
        orThrow(
          api().members.remove({
            path: { project_id: projectId, user_id: userId },
          }),
        ),
      meta: { invalidates: [userQueries.members(projectId).queryKey] },
    }),
  /**
   * Invite an email with a role, optionally delivering the link by email.
   * Asking for delivery with no SMTP source configured comes back as a
   * `validation` naming `send_email` — the form keeps its switch disabled
   * in that state. An address that already has an account or an invite comes
   * back as a `conflict` naming `email`, for the email input.
   */
  invite: () =>
    mutationOptions({
      mutationFn: (input: CreateInvitation) =>
        orThrow(api().invitations.create({ body: input })),
      meta: { invalidates: [userQueries.invitations().queryKey] },
    }),
  /**
   * Email an invitation again. Only reached for rows with no delivered email:
   * the server would demand `force` for an already-delivered one, and
   * re-mailing that is the recipient's request ("it landed in spam"), not a
   * bulk click — so the dashboard leaves force to the API, never the list.
   */
  resendInvitation: () =>
    mutationOptions({
      mutationFn: (input: { token: string }) =>
        orThrow(api().invitations.resend({ path: { token: input.token } })),
      meta: { invalidates: [userQueries.invitations().queryKey] },
    }),
  /** The token is a variable: one mutation serves every pending row. */
  revokeInvitation: () =>
    mutationOptions({
      mutationFn: (token: string) =>
        orThrow(api().invitations.revoke({ path: { token } })),
      meta: { invalidates: [userQueries.invitationsRoot()] },
    }),
};
