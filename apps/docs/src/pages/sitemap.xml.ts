import type { APIRoute } from 'astro';
import { getPosts, isoDate } from '../blog';
import { getReleases } from '../changelog';
import { PAGES } from '../docs-nav';
import { COMPETITORS } from '../lib/vs';

/**
 * `/sitemap.xml`, the URL search engines already know.
 *
 * Every static page under `src/pages`, found by its file, then the
 * documentation, the comparisons, the posts and the releases. A new dynamic route (`[slug]`) lists nothing until its paths
 * are added here. Only posts and releases carry a `lastmod`: CI checks out
 * without history, so a page's would be the build date, and Google ignores a
 * lastmod it learns not to trust.
 */
const pages = Object.keys(import.meta.glob('./**/*.astro'))
  .filter((file) => !file.includes('[') && file !== './404.astro')
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
  const comparisons = COMPETITORS.map((c) => url(`/vs/${c.slug}`));
  const docs = PAGES.map((page) => url(`/${page.slug}`));
  return new Response(
    `<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">${[...pages.map((path) => url(path)), ...docs, ...comparisons, ...posts, ...releases].join('')}</urlset>`,
    { headers: { 'Content-Type': 'application/xml' } },
  );
};
