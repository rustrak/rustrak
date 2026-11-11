import { type UseQueryOptions, useQuery } from '@tanstack/react-query';
import type { ApiError } from '@/shared/api/query';
import { usePermissionsLater } from '@/shared/ui/hooks/use-permissions';
import { useSessionUser } from '@/shared/ui/hooks/use-session-user';
import { type Assignable, assignable } from '../../model/assignable';

type Member = { user_id: number; email: string };

/**
 * Who an issue can be assigned to, the reader included. The project's members
 * (the page hands in the read, which lives in the user slice) are an extra:
 * asked only of who may read them, since anyone else gets a 403, and the
 * reader alone while they load or when they fail.
 */
export function useAssignable<
  TMember extends Member,
  TKey extends readonly unknown[],
>(
  projectId: number,
  membersRead: UseQueryOptions<TMember[], ApiError, TMember[], TKey>,
): { me: number; members: Assignable[] } {
  const user = useSessionUser();
  const canRead = usePermissionsLater().can(
    'project-members',
    'read',
    projectId,
  );
  const members =
    useQuery({
      ...membersRead,
      // The page may hold the read back too (its skeleton does).
      enabled: canRead && membersRead.enabled !== false,
    }).data ?? [];
  return { me: user.id, members: assignable(members, user) };
}
