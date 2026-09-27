/**
 * The update check, read by the banner slot.
 */
import { z } from 'zod';
import { queryClient } from '@/shared/api/query-client';
import { serverVersionQuery } from '@/shared/api/server-version';
import { compareVersions, type UpdateCheck } from '@/shared/lib/version';

const VERSIONS_URL = 'https://rustrak.github.io/rustrak/versions.json';
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
 * Whether to check at all.
 *
 * **This became a build-time switch, and that is a real change.** It used to
 * be `process.env.RUSTRAK_VERSION_CHECK_ENABLED`, read per request by the Node
 * process, so an operator could turn the check off with an environment
 * variable on a running container. A static bundle has no process to read one
 * from: what the browser can see is what the build baked in.
 *
 * Anyone who needs it off in a deployment they do not build has two answers
 * that still work, and both are better than the banner: block the outbound
 * request to GitHub Pages, or dismiss the banner, which is per-version and
 * sticky. If runtime control turns out to matter, the honest fix is for the
 * server to answer it — not for the bundle to pretend it can read the
 * environment.
 */
function isEnabled(): boolean {
  return import.meta.env.VITE_RUSTRAK_VERSION_CHECK_ENABLED !== 'false';
}

/**
 * Compare the running server against the published release feed.
 *
 * The version being compared is the **server's**, and only the server's. This
 * used to fall back to `APP_VERSION`, the frontend's own bundled version, when
 * the server could not be reached. In a deployment where the two differ (the
 * whole point of shipping the dashboard separately from the API) that answered
 * a question nobody asked, with a banner, on every page, with full confidence:
 * "an update is available" computed from a number belonging to the wrong
 * process. Same defect as a zeroed session summary. A plausible value standing
 * in for an unknown one, indistinguishable from a measured one at the point it
 * is rendered.
 *
 * So an unreadable server version ends the check. No banner is worse than a
 * wrong banner, and there is no substitute version to reach for.
 */
export async function checkForUpdate(): Promise<UpdateCheck> {
  if (!isEnabled()) return { state: 'disabled' };

  // Read outside the `try` on purpose. `@rustrak/client` failures are values,
  // not throws, and this branch is the one that has to be seen; putting the
  // read inside the block below would let a future refactor drop it into the
  // catch and quietly restore the bug this function exists to fix.
  const version = await queryClient.fetchQuery(serverVersionQuery);
  if (!version.success) return { state: 'unknown', reason: 'server-version' };

  const current = version.data.version;

  // The `try` is for this fetch alone. It is a direct call to GitHub Pages that
  // does not go through `@rustrak/client`, so DNS failures, aborts, a blocked
  // cross-origin request and a body that is not JSON all arrive as exceptions
  // and genuinely have to be caught.
  try {
    const response = await fetch(VERSIONS_URL, {
      // The browser's own HTTP cache decides how long this is good for, from
      // the headers GitHub Pages sends. Next's `revalidate` was a server-side
      // cache for a fetch a server no longer makes.
      signal: AbortSignal.timeout(FETCH_TIMEOUT_MS),
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
