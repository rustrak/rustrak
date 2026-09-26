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
} from '@rustrak/client';
import { createClient } from '@/shared/api/rustrak';

/** Public SSO configuration for the unauthenticated login page. */
export async function getSsoConfig(): Promise<Result<SsoConfig, RustrakError>> {
  const client = await createClient();
  return client.auth.getSsoConfig();
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
