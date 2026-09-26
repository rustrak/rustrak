import type {
  AcceptInvitation,
  CreateInvitation,
  GlobalRole,
  Invitation,
  LoginRequest,
  Result,
  RustrakError,
  UpsertProjectMember,
  User,
} from '@rustrak/client';
import { Ok } from '@rustrak/client';
import { createClient } from '@/shared/api/rustrak';
import { session } from '@/shared/api/session';

/**
 * Login with email and password.
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
 * **Nothing here touches the cookie any more.** Under Next this ran on the
 * server, so the `Set-Cookie` headers the API answered with had to be parsed
 * out of `result.data` and replayed onto the Next cookie store by hand — three
 * helpers and a parser, all of them a workaround for the request being made by
 * a process that was not the browser. The browser makes it now, same origin,
 * so it stores the cookie itself. `result.data.cookies` is still on the wire
 * and is simply not read.
 *
 * The session store is told directly rather than left to re-ask: the sign-in
 * response already carries the user, and a second `/auth/me` to learn what we
 * were just handed is a round trip the login screen pays for in latency.
 */
export async function login(
  credentials: LoginRequest,
): Promise<Result<User, RustrakError>> {
  const client = await createClient();
  const result = await client.auth.login(credentials);

  if (!result.success) return result;

  session.set({ state: 'authenticated', user: result.data.user });

  return { success: true, data: result.data.user };
}

/**
 * Logout the current user.
 *
 * The session this tab holds is dropped whether or not the request lands. It
 * is not a claim that the server session ended — the next guard asks and gets
 * the truth — it is a refusal to keep rendering the dashboard, which is the
 * safer of the two failures on a shared machine.
 */
export async function logout(): Promise<void> {
  const client = await createClient();

  try {
    await client.auth.logout();
  } finally {
    session.clear();
  }
}

/**
 * Accept an invitation by setting a password.
 * On success the backend creates a session, and the browser stores its cookie.
 */
export async function acceptInvitation(
  input: AcceptInvitation,
): Promise<Result<User, RustrakError>> {
  const client = await createClient();
  const result = await client.auth.acceptInvitation(input);

  if (!result.success) return result;

  session.set({ state: 'authenticated', user: result.data.user });

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
 * The updated user is published to the session store rather than discarded,
 * and that is load-bearing now. Under Next the caller followed this with
 * `router.refresh()` and the server read the row again, so both preferences
 * took effect on the next render for free. Here the row is only read at boot:
 * without this the store would still be holding the language the reader just
 * changed away from, and `intl.reload()` would resolve straight back to it.
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

  if (!result.success) return result;

  session.set({ state: 'authenticated', user: result.data });

  return Ok(undefined);
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
