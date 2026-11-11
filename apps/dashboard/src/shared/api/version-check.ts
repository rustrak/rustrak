import { type QueryClient, queryOptions } from '@tanstack/react-query';
import { z } from 'zod';
import { serverVersionQuery } from '@/shared/api/server-version';
import { compareVersions, type UpdateCheck } from '@/shared/lib/version';

const VERSIONS_URL = 'https://rustrak.dev/versions.json';
const FETCH_TIMEOUT_MS = 3000;

const feedSchema = z.object({
  versions: z.array(
    z.object({
      version: z.string(),
      description: z.string().optional(),
      url: z.string(),
    }),
  ),
});

/**
 * Whether to check at all: baked in at build time, since a static bundle has
 * no process environment to read. Without a build of their own, an operator
 * can block the outbound request or dismiss the banner (per version, sticky).
 */
function isEnabled(): boolean {
  return import.meta.env.VITE_RUSTRAK_VERSION_CHECK_ENABLED !== 'false';
}

/**
 * Compare the running server against the published release feed.
 *
 * The version compared is the **server's**, and only the server's: the
 * dashboard may be deployed apart from it, so its own bundled version would
 * answer a question nobody asked. An unreadable server version ends the check;
 * no banner is better than a wrong one.
 */
async function checkForUpdate(
  client: QueryClient,
  signal: AbortSignal,
): Promise<UpdateCheck> {
  if (!isEnabled()) return { state: 'disabled' };

  // The same cache entry as the About page's, so it is one request.
  let current: string;
  try {
    current = (await client.fetchQuery(serverVersionQuery())).version;
  } catch {
    return { state: 'unknown', reason: 'server-version' };
  }

  // A direct fetch, not `@rustrak/client`: DNS failures, aborts, a blocked
  // cross-origin request and a body that is not JSON all arrive as throws.
  try {
    const response = await fetch(VERSIONS_URL, {
      // The browser's own HTTP cache decides how long this is good for.
      signal: AbortSignal.any([signal, AbortSignal.timeout(FETCH_TIMEOUT_MS)]),
    });
    if (!response.ok) return { state: 'unknown', reason: 'feed' };

    const feed = feedSchema.safeParse(await response.json());
    if (!feed.success) return { state: 'unknown', reason: 'feed' };

    // Picks the max explicitly instead of trusting feed order: versions.json is
    // sorted by date then slug, so a backported patch released after a later
    // minor would otherwise land first.
    const latest = feed.data.versions.reduce<
      (typeof feed.data.versions)[number] | undefined
    >((newest, entry) => {
      if (compareVersions(entry.version, current) <= 0) return newest;
      if (!newest || compareVersions(entry.version, newest.version) > 0)
        return entry;
      return newest;
    }, undefined);
    if (!latest) return { state: 'up-to-date' };

    return {
      state: 'update-available',
      info: {
        current,
        latest: latest.version,
        description: latest.description ?? '',
        url: latest.url,
      },
    };
  } catch {
    return { state: 'unknown', reason: 'feed' };
  }
}

/**
 * The update check, once per tab: the running version does not change under
 * a reader, and the feed is a public file the browser's HTTP cache already
 * governs. It never fails; every way of not knowing is the `unknown` state.
 */
export function updateCheckQuery() {
  return queryOptions({
    queryKey: ['update-check'],
    queryFn: ({ client, signal }) => checkForUpdate(client, signal),
    staleTime: Number.POSITIVE_INFINITY,
  });
}
