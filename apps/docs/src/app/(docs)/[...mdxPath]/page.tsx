import { generateStaticParamsFor, importPage } from 'nextra/pages';
import { useMDXComponents } from '../../../../mdx-components';

/**
 * Every MDX page in `content/`, except the ones with a hand-written route.
 *
 * ── A required catch-all, not an optional one ───────────────────────────────
 *
 * This was `[[...mdxPath]]`, which also matches `/`. That is how the landing
 * used to be served: as `content/index.mdx`, through this same pipeline. Now
 * that `/` has its own route (`app/page.tsx`) the two would collide — Next
 * refuses a page and an optional catch-all at the same specificity — so the
 * brackets lost a pair and this no longer answers for the root.
 */
export async function generateStaticParams() {
  const all = await generateStaticParamsFor('mdxPath')();
  return all.filter((p) => {
    // The root. Served by `app/page.tsx` as the landing, and unreachable
    // through a required catch-all anyway — generating it here would be a
    // second page claiming the same path.
    if (!Array.isArray(p.mdxPath) || p.mdxPath.length === 0) {
      return false;
    }
    // Blog posts are handled by app/(docs)/blog/[slug]/page.tsx
    // Changelog posts are handled by app/(docs)/changelog/page.tsx
    return p.mdxPath[0] !== 'blog' && p.mdxPath[0] !== 'changelog';
  });
}

// Return 404 for paths not in generateStaticParams
export const dynamicParams = false;

export async function generateMetadata(props: PageProps) {
  const params = await props.params;
  const { metadata } = await importPage(params.mdxPath);
  // The page's Markdown twin, written by scripts/generate-llms.mjs, so an agent
  // reading the HTML can find the clean version (llmstxt.org, v2).
  return {
    ...metadata,
    alternates: {
      types: { 'text/markdown': `/${params.mdxPath.join('/')}.md` },
    },
  };
}

type PageProps = {
  params: Promise<{ mdxPath: string[] }>;
};

export default async function Page(props: PageProps) {
  const params = await props.params;
  const result = await importPage(params.mdxPath);
  const { default: MDXContent, toc, metadata } = result;
  const Wrapper = useMDXComponents().wrapper;

  return (
    <Wrapper toc={toc} metadata={metadata}>
      <MDXContent {...props} params={params} />
    </Wrapper>
  );
}
