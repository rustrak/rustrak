import type { APIRoute, GetStaticPaths } from 'astro';
import { type DocPage, getDocs, slugOf } from '../docs';
import { docMarkdown, SITE } from '../llms';

/**
 * `/<page>.md`: the page as Markdown, for "Copy page", for assistants, and
 * for `llms.txt`: the page's URL in
 * frontmatter, then its body, which already opens with its `# Title`. The
 * one component the pages use, `<InteractiveDemo>`, means nothing as text and
 * is left out.
 */
export const getStaticPaths = (async () =>
  (await getDocs()).map((page) => ({
    params: { slug: slugOf(page) },
    props: { page },
  }))) satisfies GetStaticPaths;

export const GET: APIRoute<{ page: DocPage }> = ({ props: { page } }) =>
  new Response(
    `---\nurl: ${SITE}/${slugOf(page)}\n---\n\n${docMarkdown(page)}\n`,
    { headers: { 'Content-Type': 'text/markdown; charset=utf-8' } },
  );
