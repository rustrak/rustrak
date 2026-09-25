import { getPageMap } from 'nextra/page-map';
import { Footer, Layout, Navbar } from 'nextra-theme-docs';
import { RustrakWordmark } from '@/components/icons/rustrak-wordmark';

/**
 * The documentation shell: navbar, sidebar, search, table of contents, footer.
 *
 * This used to be the root layout, which meant it wrapped every route in the
 * app — including the landing, which uses none of it. A visitor arriving at `/`
 * was downloading and hydrating a documentation theme to look at a marketing
 * page, and the landing was then spending three blocks of CSS in `globals.css`
 * undoing the layout it had been given (the prose width cap, the trailing
 * "last updated" spacer, the content width variable). All of that is gone: the
 * landing is a sibling route now and simply never enters this tree.
 *
 * `getPageMap()` moving here matters as much as the theme did. It is the whole
 * navigable structure of the docs, serialised into the payload of whatever
 * renders it — and it was being serialised into the landing's HTML too, for a
 * page with no sidebar to put it in.
 *
 * This is a route group, so `(docs)` contributes nothing to any URL. Everything
 * inside keeps the path it always had.
 */

/* Placed, not typed. See the note at the top of `icons/rustrak-wordmark.tsx`:
   the word is a drawing, and typing it makes the mark depend on which font the
   browser resolved. 20px is the brand's floor for a product header. */
const logo = <RustrakWordmark className="h-5 w-auto" />;

export default async function DocsLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const pageMap = await getPageMap();

  return (
    <Layout
      pageMap={pageMap}
      docsRepositoryBase="https://github.com/AbianS/rustrak/tree/main/apps/docs"
      navbar={
        <Navbar logo={logo} projectLink="https://github.com/AbianS/rustrak" />
      }
      footer={<Footer>GPL-3.0 {new Date().getFullYear()} Rustrak</Footer>}
    >
      {children}
    </Layout>
  );
}
