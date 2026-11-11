import { queryOptions } from '@tanstack/react-query';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';

/**
 * The version the server reports for itself.
 *
 * A failure is a failure, not `null`. The About page can render
 * "unavailable" in a row and lose nothing; the update check cannot: it
 * compares this number against a published feed and puts a banner on every
 * page from the result, so "we never found out" must stay distinguishable
 * from any version.
 */
export function serverVersionQuery() {
  return queryOptions({
    queryKey: ['server-version'],
    queryFn: ({ signal }) => orThrow(api().health.version({ signal })),
  });
}
