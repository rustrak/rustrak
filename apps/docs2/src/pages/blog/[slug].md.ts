import type { APIRoute, GetStaticPaths } from 'astro';
import { getPosts, type Post } from '../../blog';

/**
 * `/blog/<slug>.md`: the post as plain Markdown, for assistants and anything
 * else that reads text rather than pages. `llms.txt` links here, and each post
 * points at it from its `<head>`. The same shape the current site writes.
 */
export const getStaticPaths = (async () =>
  (await getPosts()).map((post) => ({
    params: { slug: post.id },
    props: { post },
  }))) satisfies GetStaticPaths;

export const GET: APIRoute<{ post: Post }> = ({ props: { post }, site }) => {
  const url = new URL(`/blog/${post.id}`, site);
  const body = `---\nurl: ${url}\n---\n\n# ${post.data.title}\n\n${post.body?.trim()}\n`;
  return new Response(body, {
    headers: { 'Content-Type': 'text/markdown; charset=utf-8' },
  });
};
