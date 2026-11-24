import type { APIRoute } from 'astro';

/**
 * `/sitemap.xml`, at the URL the current site already gave search engines.
 *
 * Every static page under `src/pages`, found by its file. A dynamic route
 * (`[slug]`) lists nothing here yet: add its paths when one is ported.
 */
const pages = Object.keys(import.meta.glob('./**/*.astro'))
  .filter((file) => !file.includes('['))
  .map((file) => file.slice(1).replace(/(\/index)?\.astro$/, '') || '/');

export const GET: APIRoute = ({ site }) => {
  const urls = pages
    .map((path) => `<url><loc>${new URL(path, site)}</loc></url>`)
    .join('');
  return new Response(
    `<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">${urls}</urlset>`,
    { headers: { 'Content-Type': 'application/xml' } },
  );
};
