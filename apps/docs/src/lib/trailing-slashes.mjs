import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * GitHub Pages serves every page as a folder and 301s `/x` to `/x/`, the
 * canonical. Links are written without the slash all over (components, nav
 * data, 200+ in the Markdown), so instead of chasing each one this adds it to
 * every internal page link in the built HTML. A path whose last segment has a
 * dot is a file (`/rss.xml`, `/x.md`) and keeps its form. Absolute links to
 * `site` itself (the changelog has some) are internal too.
 */
export const fixHref = (html, site) => {
  const origin = new URL(site).origin.replaceAll('.', '\\.');
  const link = new RegExp(
    `href="((?:${origin})?/(?!/)[^"#?]*)([?#][^"]*)?"`,
    'g',
  );
  return html.replace(link, (all, p, rest = '') =>
    p.endsWith('/') || path.posix.basename(p).includes('.')
      ? all
      : `href="${p}/${rest}"`,
  );
};

/**
 * A feed item's `<guid>`, which `@astrojs/rss` takes from its link. The feeds
 * were published with unslashed links, and a reader shows an item whose guid
 * changed as new, so the guid keeps that form while the link gains the slash.
 */
export const legacyGuid = (path, site) =>
  `<guid isPermaLink="true">${new URL(path, site)}</guid>`;

export const trailingSlashes = () => {
  let site;
  return {
    name: 'trailing-slashes',
    hooks: {
      'astro:config:done': ({ config }) => {
        site = config.site;
      },
      'astro:build:done': async ({ dir }) => {
        const root = fileURLToPath(dir);
        for (const file of await fs.readdir(root, { recursive: true })) {
          if (!file.endsWith('.html')) continue;
          const full = path.join(root, file);
          const html = await fs.readFile(full, 'utf8');
          const fixed = fixHref(html, site);
          if (fixed !== html) await fs.writeFile(full, fixed);
        }
      },
    },
  };
};
