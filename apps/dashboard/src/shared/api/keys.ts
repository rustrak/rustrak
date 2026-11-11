/**
 * The first segment of every query key that belongs to one project.
 *
 * Slices cannot import each other, and a project's issues, agents, releases
 * and alerts all need the same prefix: with it, invalidating a project's
 * scope refreshes everything of that project, and deleting one drops it all.
 */
export function projectScope(projectId: number) {
  return [...allProjects(), projectId] as const;
}

/** The root of every project read: lists and each project's scope. */
export function allProjects() {
  return ['projects'] as const;
}

/**
 * Whether the server can send invitation emails. Read by the user slice and
 * invalidated by the alert slice, since an email integration is what sends
 * them, so the key lives below both.
 */
export function invitationEmailStatus() {
  return ['invitations', 'email-status'] as const;
}
