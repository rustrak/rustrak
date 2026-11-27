import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * GitHub Pages serves every page as a folder and 301s `/x` to `/x/`, the
 * canonical. Links are written without the slash all over (components, nav
 * data, 200+ in the Markdown), so instead of chasing each one this adds it to
 * every internal page link in the built HTML. A path whose last segment has a
 * dot is a file (`/rss.xml`, `/x.md`) and keeps its form.
 */
export const fixHref = (html) =>
  html.replace(/href="(\/(?!\/)[^"#?]*)([?#][^"]*)?"/g, (all, p, rest = '') =>
    p.endsWith('/') || path.posix.basename(p).includes('.')
      ? all
      : `href="${p}/${rest}"`,
  );

export const trailingSlashes = () => ({
  name: 'trailing-slashes',
  hooks: {
    'astro:build:done': async ({ dir }) => {
      const root = fileURLToPath(dir);
      for (const file of await fs.readdir(root, { recursive: true })) {
        if (!file.endsWith('.html')) continue;
        const full = path.join(root, file);
        const html = await fs.readFile(full, 'utf8');
        const fixed = fixHref(html);
        if (fixed !== html) await fs.writeFile(full, fixed);
      }
    },
  },
});
