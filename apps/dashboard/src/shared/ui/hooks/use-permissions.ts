import { useQuery, useSuspenseQuery } from '@tanstack/react-query';
import { accessQueries } from '@/shared/api/permissions';
import {
  NO_PERMISSIONS,
  type Permissions,
  permissionsFrom,
} from '@/shared/lib/permissions';

/**
 * What the reader may do, waiting for the answer: for pages whose content
 * depends on it, behind their own skeleton.
 */
export function usePermissions(): Permissions {
  const { data } = useSuspenseQuery(accessQueries.me());
  return permissionsFrom(data);
}

/**
 * The same, without waiting: nothing is allowed until the answer lands. For
 * the frame, which hides an entry for a moment rather than blanking.
 */
export function usePermissionsLater(): Permissions {
  const { data } = useQuery(accessQueries.me());
  return permissionsFrom(data ?? NO_PERMISSIONS);
}
