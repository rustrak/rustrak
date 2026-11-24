import type { APIRoute } from 'astro';
import { getPosts } from '../blog';
import { getDocs, slugOf } from '../docs';
import { PAGES } from '../docs-nav';
import { benchmarkMarkdown, docMarkdown, postMarkdown, SITE } from '../llms';

/** `/llms-full.txt`: every docs page, post and the benchmark in one file. */
export const GET: APIRoute = async () => {
  const docs = new Map((await getDocs()).map((page) => [slugOf(page), page]));
  const parts = [
    ...PAGES.flatMap((item) => {
      const page = docs.get(item.slug);
      return page
        ? [`<!-- ${SITE}/${item.slug} -->\n\n${docMarkdown(page)}`]
        : [];
    }),
    ...(await getPosts()).map(
      (post) => `<!-- ${SITE}/blog/${post.id} -->\n\n${postMarkdown(post)}`,
    ),
    `<!-- ${SITE}/benchmarks -->\n\n${benchmarkMarkdown()}`,
  ];
  return new Response(
    `# Rustrak documentation\n\n${parts.join('\n\n---\n\n')}\n`,
    { headers: { 'Content-Type': 'text/plain; charset=utf-8' } },
  );
};
