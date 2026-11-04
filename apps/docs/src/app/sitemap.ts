import fs from 'node:fs';
import path from 'node:path';
import type { MetadataRoute } from 'next';
import { getPosts } from '@/lib/blog';
import { SITE } from '@/lib/seo';

export const dynamic = 'force-static';

const CONTENT = path.join(process.cwd(), 'content');

/**
 * The URL of every docs page, from the files in `content/`. Blog and changelog
 * have their own routes and are listed separately below. `index.mdx` answers
 * for its folder, as it does in Nextra.
 */
function docsPaths(dir = ''): string[] {
  return fs
    .readdirSync(path.join(CONTENT, dir), { withFileTypes: true })
    .flatMap((entry) => {
      const rel = path.posix.join(dir, entry.name);
      if (entry.isDirectory()) {
        return rel === 'blog' || rel === 'changelog' ? [] : docsPaths(rel);
      }
      if (!entry.name.endsWith('.mdx')) return [];
      return [`/${rel.replace(/(\/index)?\.mdx$/, '')}`];
    });
}

/**
 * No `lastModified` on docs pages: CI checks out without history, so every
 * file would claim the build date, and Google ignores a lastmod it learns not
 * to trust. Posts carry their real publication date.
 */
export default function sitemap(): MetadataRoute.Sitemap {
  const pages = ['/', '/blog', '/changelog', '/roadmap', '/api-reference'];
  return [
    ...pages.map((p) => ({ url: `${SITE}${p === '/' ? '' : p}` })),
    ...docsPaths()
      .sort()
      .map((p) => ({ url: `${SITE}${p}` })),
    ...getPosts().map((post) => ({
      url: `${SITE}/blog/${post.slug}`,
      lastModified: post.date,
    })),
  ];
}
