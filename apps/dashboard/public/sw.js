/*
 * The service worker. It makes the dashboard installable and keeps it opening
 * on a bad connection; it never makes it work offline, and that is deliberate.
 *
 * What it does, by request:
 *
 * - `/assets/*` is cache-first. Vite fingerprints every file there and the
 *   server marks them `immutable`, so a cached copy is never stale.
 * - A navigation goes to the network, always. The server sends the shell with
 *   `no-store` so a deploy never leaves a tab pointing at chunks that are gone
 *   and Back after a logout never shows a signed-in page; serving a cached
 *   shell here would undo both. Only when the network fails does it answer,
 *   with `offline.html`.
 * - Everything else -- the API, `/auth` (SSO is a top-level navigation, and
 *   is matched below before the navigation rule), the docs -- is not touched:
 *   no `respondWith`, so the browser handles it as if there were no worker. An
 *   error tracker's data is live, and caching a signed-in response would show
 *   it to whoever signs in next on the same device.
 *
 * The cache is named after the release (`?v=` on the registration URL), so a
 * new release installs a new worker, and activating it drops the old assets.
 */

const VERSION = new URL(self.location.href).searchParams.get('v') || 'dev';
const CACHE = `rustrak-${VERSION}`;
const OFFLINE = '/offline.html';

/** What the Rust server answers itself; the same list as `vite.config.ts`. */
const SERVER_PREFIXES = [
  '/api',
  '/auth',
  '/health',
  '/docs',
  '/api-docs',
  '/metrics',
  '/mcp',
  '/.well-known',
];

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) => cache.add(new Request(OFFLINE, { cache: 'reload' })))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    (async () => {
      if (self.registration.navigationPreload) {
        await self.registration.navigationPreload.enable();
      }
      const names = await caches.keys();
      await Promise.all(
        names
          .filter((name) => name !== CACHE)
          .map((name) => caches.delete(name)),
      );
      await self.clients.claim();
    })(),
  );
});

const isServerPath = (pathname) =>
  SERVER_PREFIXES.some(
    (prefix) => pathname === prefix || pathname.startsWith(`${prefix}/`),
  );

self.addEventListener('fetch', (event) => {
  const { request } = event;
  if (request.method !== 'GET') return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin || isServerPath(url.pathname)) return;

  if (url.pathname.startsWith('/assets/')) {
    event.respondWith(cacheFirst(request));
  } else if (request.mode === 'navigate') {
    event.respondWith(networkOrOffline(event));
  }
});

async function cacheFirst(request) {
  const cache = await caches.open(CACHE);
  const hit = await cache.match(request);
  if (hit) return hit;
  const response = await fetch(request);
  if (response.ok && response.type === 'basic') {
    cache.put(request, response.clone());
  }
  return response;
}

async function networkOrOffline(event) {
  try {
    return (await event.preloadResponse) || (await fetch(event.request));
  } catch {
    const cache = await caches.open(CACHE);
    return (await cache.match(OFFLINE)) || Response.error();
  }
}
