import rss from '@astrojs/rss';
import type { APIRoute } from 'astro';
import { getReleases } from '../../changelog';

/**
 * Every release as a feed item, with its notes in full: someone who
 * subscribes to a changelog wants what changed, not a link to it.
 */
export const GET: APIRoute = async ({ site }) =>
  rss({
    title: 'Rustrak Changelog',
    description: 'Release notes and version history for the Rustrak project.',
    site: site ?? 'https://rustrak.dev',
    trailingSlash: false,
    items: (await getReleases()).map((release) => ({
      title: `${release.version}: ${release.title}`,
      description: release.description,
      pubDate: release.date,
      categories: release.tags,
      link: `/changelog/${release.anchor}`,
      content: release.sections
        .map(({ title, html }) => (title ? `<h2>${title}</h2>${html}` : html))
        .join(''),
    })),
  });
