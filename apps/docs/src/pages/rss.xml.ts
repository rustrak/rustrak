import rss from '@astrojs/rss';
import type { APIRoute } from 'astro';
import { getPosts } from '../blog';

/** The blog as a feed, newest first. Every page links it from its `<head>`. */
export const GET: APIRoute = async ({ site }) =>
  rss({
    title: 'Rustrak Blog',
    description:
      'Updates, releases, and engineering notes from the Rustrak project.',
    site: site ?? 'https://rustrak.dev',
    // The canonical URLs end in one: GitHub Pages serves each page as a folder.
    trailingSlash: true,
    items: (await getPosts()).map((post) => ({
      title: post.data.title,
      description: post.data.description,
      pubDate: post.data.date,
      categories: post.data.tags,
      link: `/blog/${post.id}`,
    })),
  });
