'use server';

import type {
  AcceptInvitation,
  CreateInvitation,
  GlobalRole,
  Invitation,
  LoginRequest,
  Result,
  RustrakError,
  TeamMember,
  UpsertProjectMember,
  User,
} from '@rustrak/client';
import { Ok } from '@rustrak/client';
import {
  applySetCookies,
  clearSessionCookies,
  createClient,
  dropSessionCookie,
} from '@/shared/api/rustrak';
import { listTeam as listTeamQuery } from './queries';

/**
 * Login with email and password, applying the session cookies on success.
 *
 * Returns the client's own `Result` like every other action here. It used to
 * translate the failure into a domain enum, and that layer bought nothing: the
 * server answers the same `Unauthorized` whether the email is unknown, the
 * password is wrong, or the account is disabled, so the client already reports
 * a single `unauthenticated` and there is nothing left to collapse.
 *
 * Deciding that `unauthenticated` deserves one deliberately vague sentence is a
 * *presentation* decision and lives in the form, next to the other copy. See
 * `login-form.tsx`, which explains why that vagueness is deliberate.
 *
 * The success arm is narrowed to the user on purpose: `result.data` also holds
 * the `Set-Cookie` headers, which are applied here and must not travel to a
 * Client Component.
 */
export async function login(
  credentials: LoginRequest,
): Promise<Result<User, RustrakError>> {
  const client = await createClient();
  const result = await client.auth.login(credentials);

  if (!result.success) return result;

  await applySetCookies(result.data.cookies);

  return { success: true, data: result.data.user };
}

/** Start OIDC login, persisting the encrypted state/PKCE session cookie. */
export async function startSso(): Promise<Result<string, RustrakError>> {
  const client = await createClient();
  const result = await client.auth.startSso();

  if (!result.success) return result;

  await applySetCookies(result.data.cookies);
  return Ok(result.data.authorizationUrl);
}

/**
 * Logout the current user.
 * Clears the session cookie.
 */
export async function logout(): Promise<void> {
  const client = await createClient();
  const result = await client.auth.logout();

  if (!result.success) {
    // The server did not acknowledge, so there are no `Set-Cookie` headers to
    // replay. Drop the cookie anyway: a logout that leaves the session cookie
    // in place is a session the user believes has ended and has not, which is
    // the worse of the two failures on a shared machine.
    await dropSessionCookie();
    return;
  }

  await clearSessionCookies(result.data);
}

/**
 * Accept an invitation by setting a password.
 * On success the backend creates a session; the cookie is persisted here.
 *
 * Narrowed to the user for the same reason as {@link login}: `result.data` also
 * carries the `Set-Cookie` headers, which are applied here and must not reach a
 * Client Component.
 */
export async function acceptInvitation(
  input: AcceptInvitation,
): Promise<Result<User, RustrakError>> {
  const client = await createClient();
  const result = await client.auth.acceptInvitation(input);

  if (!result.success) return result;

  await applySetCookies(result.data.cookies);

  return { success: true, data: result.data.user };
}

/**
 * Update a user's global role (admin/member).
 *
 * The backend guards against removing the last admin and against
 * unauthorized callers (only instance admins may change roles).
 *
 * The `RustrakError` is returned rather than flattened to a string, because the
 * server names `role` on the rejections it can attribute and the caller needs
 * `fields` to put the message on the control the user just changed.
 *
 * @param userId - The user ID to update
 * @param role - The new global role
 * @returns `Ok` on success, or the failure the server reported
 */
export async function updateUserRole(
  userId: number,
  role: GlobalRole,
): Promise<Result<void, RustrakError>> {
  const client = await createClient();
  return client.team.updateRole(userId, role);
}

/**
 * Store how this reader wants the dashboard presented to them.
 *
 * A field left out is left alone; a field sent as `null` is cleared, which is
 * how a reader goes back to "infer it from my browser" after having chosen.
 *
 * @param preferences - Language and/or timezone
 * @returns `Ok` on success, or the failure the server reported
 */
export async function updatePreferences(preferences: {
  language?: string | null;
  timezone?: string | null;
}): Promise<Result<void, RustrakError>> {
  const client = await createClient();
  const result = await client.auth.updatePreferences(preferences);
  // The updated user comes back and nothing here needs it: the page re-renders
  // from the server, which reads the row again.
  return result.success ? Ok(undefined) : result;
}

/**
 * Permanently remove a user from the instance.
 *
 * The backend guards against deleting yourself, the primary user, and the
 * last remaining admin.
 *
 * @param userId - The user ID to remove
 * @returns `Ok` on success, or the failure the server reported
 */
export async function removeTeamMember(
  userId: number,
): Promise<Result<void, RustrakError>> {
  const client = await createClient();
  return client.team.remove(userId);
}

/**
 * Add or update a member of a project (upsert by user_id).
 *
 * The backend guards against unauthorized callers (only global admins
 * or project admins may manage members) and returns 403 otherwise.
 *
 * The `RustrakError` is returned rather than flattened to a string: the server
 * names `role` on a rejected role, and only the caller can decide which control
 * that belongs on.
 *
 * @param projectId - The project ID
 * @param input - The user_id and per-project role
 * @returns `Ok` on success, or the failure the server reported
 */
export async function upsertProjectMember(
  projectId: number,
  input: UpsertProjectMember,
): Promise<Result<void, RustrakError>> {
  const client = await createClient();
  return client.members.upsert(projectId, input);
}

/**
 * Remove a member from a project.
 *
 * @param projectId - The project ID
 * @param userId - The user ID to remove
 * @returns `Ok` on success, or the failure the server reported
 */
export async function removeProjectMember(
  projectId: number,
  userId: number,
): Promise<Result<void, RustrakError>> {
  const client = await createClient();
  return client.members.remove(projectId, userId);
}

/**
 * Create a new invitation for a given email + role.
 *
 * v1 does not send email, so the returned invitation token must be
 * shared manually by the admin (used to build the invite link).
 *
 * The `RustrakError` is returned rather than flattened to a string: an address
 * that already has an account or a pending invite comes back as a `conflict`
 * naming `email`, and the form puts that on the email input instead of in a
 * toast the user has to translate back into an edit.
 *
 * @param input - Email and role for the invitee
 * @returns The created invitation, or the failure the server reported
 */
export async function createInvitation(
  input: CreateInvitation,
): Promise<Result<Invitation, RustrakError>> {
  const client = await createClient();
  return client.invitations.create(input);
}

/**
 * Revoke (delete) a pending invitation by its token.
 *
 * @param token - The invitation token to revoke
 * @returns `Ok` on success, or the failure the server reported
 */
export async function revokeInvitation(
  token: string,
): Promise<Result<void, RustrakError>> {
  const client = await createClient();
  return client.invitations.revoke(token);
}

/**
 * The add-member dropdown needs the roster from a Client Component, and
 * `queries.ts` carries `server-only`, so it cannot be reached from one.
 *
 * A thin delegate, deliberately not a re-export: whether a re-export keeps its
 * `'use server'` semantics depends on which layer is looking, and the SWC
 * transform and the TypeScript plugin disagree. Holding it as a convention
 * beats inferring it from a compiler that has two opinions.
 */
export async function listTeam(): Promise<Result<TeamMember[], RustrakError>> {
  return listTeamQuery();
}
