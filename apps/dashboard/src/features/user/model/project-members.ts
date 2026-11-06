import type { MemberRole } from '@rustrak/client';

/**
 * The roles held on this project alone: all that can be taken away here. One
 * held on every project stays, whatever is done to this project's members.
 */
export function ownRoles(member: { roles: MemberRole[] }): MemberRole[] {
  return member.roles.filter((role) => !role.inherited);
}
