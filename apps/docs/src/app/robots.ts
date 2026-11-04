import type { MetadataRoute } from 'next';
import { SITE } from '@/lib/seo';

export const dynamic = 'force-static';

/**
 * Everything is open to every crawler, AI ones included: being retrievable is
 * what gets a page cited by ChatGPT, Claude or Perplexity, and a docs site has
 * nothing to hide from them.
 */
export default function robots(): MetadataRoute.Robots {
  return {
    rules: { userAgent: '*', allow: '/' },
    sitemap: `${SITE}/sitemap.xml`,
  };
}
