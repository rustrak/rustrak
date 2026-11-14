import { Wordmark } from '@rustrak/ui';
import { getPageMap } from 'nextra/page-map';
import { Layout, Navbar } from 'nextra-theme-docs';
import { Footer } from '@/components/home/chrome';

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

/* The same wordmark the landing's nav draws, held still (no hover motion). */
const logo = <Wordmark still className="h-5 w-auto text-fg" />;

export default async function DocsLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  // The changelog keeps its navbar link but loses its ~80 entries: the theme
  // renders the whole page map into every docs page (mobile menu included),
  // so they were the first thing in each page's HTML, ahead of its content.
  // The changelog has its own index and needs none of them.
  // Everything but the children stays, the title included: without it the
  // navbar link renders empty.
  const pageMap = (await getPageMap()).map((item) => {
    if (!('children' in item) || item.name !== 'changelog') return item;
    const { children: _, ...folder } = item;
    return folder;
  });

  return (
    <Layout
      pageMap={pageMap}
      docsRepositoryBase="https://github.com/rustrak/rustrak/tree/main/apps/docs"
      navbar={
        <Navbar logo={logo} projectLink="https://github.com/rustrak/rustrak" />
      }
      footer={<Footer />}
      // Dark only, like the landing: no theme switch, and the stored or
      // system preference is ignored.
      darkMode={false}
      nextThemes={{ forcedTheme: 'dark', defaultTheme: 'dark' }}
    >
      {children}
    </Layout>
  );
}
