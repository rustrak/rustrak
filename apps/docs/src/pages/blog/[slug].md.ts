import type { APIRoute, GetStaticPaths } from 'astro';
import { getPosts, type Post } from '../../blog';
import { postMarkdown, SITE } from '../../llms';

/**
 * `/blog/<slug>.md`: the post as plain Markdown, for assistants and anything
 * else that reads text rather than pages. `llms.txt` links here, and each post
 * points at it from its `<head>`.
 */
export const getStaticPaths = (async () =>
  (await getPosts()).map((post) => ({
    params: { slug: post.id },
    props: { post },
  }))) satisfies GetStaticPaths;

export const GET: APIRoute<{ post: Post }> = ({ props: { post } }) =>
  new Response(
    `---\nurl: ${SITE}/blog/${post.id}\n---\n\n${postMarkdown(post)}\n`,
    { headers: { 'Content-Type': 'text/markdown; charset=utf-8' } },
  );
