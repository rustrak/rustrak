import { defineCollection } from 'astro:content';
import { glob } from 'astro/loaders';
import { z } from 'astro/zod';

/**
 * The blog. The schema is the frontmatter the posts already use; a post missing a title
 * or a date fails the build instead of shipping as "Untitled".
 */
const blog = defineCollection({
  loader: glob({ pattern: '*.mdx', base: './content/blog' }),
  schema: z.object({
    title: z.string(),
    description: z.string(),
    date: z.coerce.date(),
    author: z.string().default('Rustrak Team'),
    tags: z.array(z.string()).default([]),
    draft: z.boolean().default(false),
  }),
});

/**
 * The changelog: one file per release, written when the release is cut
 * (`NN-vX-Y-Z-slug.mdx`). `release.yml` also reads these files, by their
 * `version:` line, for the GitHub release body.
 */
const changelog = defineCollection({
  loader: glob({ pattern: '*.mdx', base: './content/changelog' }),
  schema: z.object({
    version: z.string(),
    title: z.string(),
    description: z.string().default(''),
    date: z.coerce.date(),
    tags: z.array(z.string()).default([]),
  }),
});

/**
 * The documentation pages: every folder of `content/` but the blog and the changelog. A page's id is its path, which
 * is its URL and its key in `docs-nav.ts`.
 */
const docs = defineCollection({
  loader: glob({
    pattern:
      '{getting-started,configuration,usage,plugins,sdks,troubleshooting,upgrading,reference}/**/*.mdx',
    base: './content',
  }),
  schema: z.object({
    title: z.string().optional(),
    description: z.string(),
    /** Numbers the page's sections as steps: a walkthrough, read in order. */
    steps: z.boolean().default(false),
  }),
});

export const collections = { blog, changelog, docs };
