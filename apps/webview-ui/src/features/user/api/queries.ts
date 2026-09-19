import 'server-only';

/**
 * Reads about people and their access.
 *
 * `auth`, `team`, `members` and `invitations` are one slice because all four
 * answer the same question from different angles: who is this, who is on the
 * instance, who is on this project, and who has been asked to join.
 */
import type {
  Invitation,
  InvitationInfo,
  ProjectMember,
  Result,
  RustrakError,
  SsoConfig,
  TeamMember,
  User,
} from '@rustrak/client';
import { createClient } from '@/shared/api/rustrak';

/** Public SSO configuration for the unauthenticated login page. */
export async function getSsoConfig(): Promise<Result<SsoConfig, RustrakError>> {
  const client = await createClient();
  return client.auth.getSsoConfig();
}

/**
 * Whether there is a session, and if not, why not.
 *
 * Three states, not a nullable user. `anonymous` is the *only* one that may
 * send the visitor to `/auth/login`: an unreachable API or a 5xx is
 * `unavailable`, and redirecting on those turns a flaky connection into a
 * login loop that logging in cannot fix, because the next request fails the
 * same way. A 403 is `unavailable` too, since it means "signed in, not
 * allowed", which login also does not fix.
 */
export type CurrentUser =
  | { state: 'authenticated'; user: User }
  | { state: 'anonymous' }
  | { state: 'unavailable'; error: RustrakError };

/**
 * Get the currently authenticated user.
 *
 * @returns Which of the three states the session is in
 */
export async function getCurrentUser(): Promise<CurrentUser> {
  const client = await createClient();
  const result = await client.auth.getCurrentUser();

  if (result.success) {
    return { state: 'authenticated', user: result.data };
  }

  if (result.error.kind === 'unauthenticated') {
    return { state: 'anonymous' };
  }

  return { state: 'unavailable', error: result.error };
}

/**
 * Fetch public information about an invitation by its token.
 * Used by the public accept-invitation page (no auth required).
 *
 * The three kinds that mean "this token buys you nothing" -- `not_found`,
 * `validation` and `gone`, for never issued, malformed, and expired or already
 * used -- are no longer folded into one verdict here. The page branches on
 * `kind` itself, which keeps the distinction available to whoever wants it and
 * keeps this action a pass-through like the other 74.
 */
export async function getInvitation(
  token: string,
): Promise<Result<InvitationInfo, RustrakError>> {
  const client = await createClient();
  return client.auth.getInvitation(token);
}

/**
 * List all team members (instance users).
 *
 * @returns List of team members with their global role and status
 */
export async function listTeam(): Promise<Result<TeamMember[], RustrakError>> {
  const client = await createClient();
  return client.team.list();
}

/**
 * List members of a project with their per-project role.
 *
 * @param projectId - The project ID
 * @returns List of project members
 */
export async function listProjectMembers(
  projectId: number,
): Promise<Result<ProjectMember[], RustrakError>> {
  const client = await createClient();
  return client.members.list(projectId);
}

/**
 * List all pending/expired invitations.
 *
 * @returns List of invitations
 */
export async function listInvitations(): Promise<
  Result<Invitation[], RustrakError>
> {
  const client = await createClient();
  return client.invitations.list();
}
