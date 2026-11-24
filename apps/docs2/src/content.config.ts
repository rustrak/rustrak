import { defineCollection } from 'astro:content';
import { glob } from 'astro/loaders';
import { z } from 'astro/zod';

/**
 * The blog, read from where the current site keeps it, so both sites publish
 * the same posts until this one replaces the other. Move the folder here then.
 *
 * The schema is the frontmatter the posts already use; a post missing a title
 * or a date fails the build instead of shipping as "Untitled".
 */
const blog = defineCollection({
  loader: glob({ pattern: '*.mdx', base: '../docs/content/blog' }),
  schema: z.object({
    title: z.string(),
    description: z.string(),
    date: z.coerce.date(),
    author: z.string().default('Rustrak Team'),
    tags: z.array(z.string()).default([]),
    draft: z.boolean().default(false),
  }),
});

export const collections = { blog };
