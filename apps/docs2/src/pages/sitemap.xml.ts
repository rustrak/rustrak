import type { APIRoute } from 'astro';
import { getPosts, isoDate } from '../blog';
import { getReleases } from '../changelog';

/**
 * `/sitemap.xml`, at the URL the current site already gave search engines.
 *
 * Every static page under `src/pages`, found by its file, then the posts and
 * the releases. A new dynamic route (`[slug]`) lists nothing until its paths
 * are added here. Only posts and releases carry a `lastmod`: CI checks out
 * without history, so a page's would be the build date, and Google ignores a
 * lastmod it learns not to trust.
 */
const pages = Object.keys(import.meta.glob('./**/*.astro'))
  .filter((file) => !file.includes('['))
  .map((file) => file.slice(1).replace(/(\/index)?\.astro$/, '') || '/');

export const GET: APIRoute = async ({ site }) => {
  const url = (path: string, lastmod?: string) =>
    `<url><loc>${new URL(path, site)}</loc>${lastmod ? `<lastmod>${lastmod}</lastmod>` : ''}</url>`;
  const posts = (await getPosts()).map((post) =>
    url(`/blog/${post.id}`, isoDate(post.data.date)),
  );
  const releases = (await getReleases()).map((release) =>
    url(`/changelog/${release.anchor}`, isoDate(release.date)),
  );
  return new Response(
    `<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">${[...pages.map((path) => url(path)), ...posts, ...releases].join('')}</urlset>`,
    { headers: { 'Content-Type': 'application/xml' } },
  );
};
