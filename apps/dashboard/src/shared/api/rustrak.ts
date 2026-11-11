import { RustrakClient } from '@rustrak/client';

/**
 * One client for the whole application, built against this page's own
 * origin. That is the design, not a fallback: the server that hands out the
 * dashboard answers `/api` too (or Vite and nginx proxy it), so the session
 * cookie stays first-party and nothing is cross-origin. See the dashboard's
 * CLAUDE.md.
 *
 * Built on first use rather than at import: `window` does not exist in the
 * Node process that runs the tests.
 */
let client: RustrakClient | undefined;

export function api(): RustrakClient {
  client ??= new RustrakClient({ baseUrl: window.location.origin });
  return client;
}
