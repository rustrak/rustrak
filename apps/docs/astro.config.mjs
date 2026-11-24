import { rehypeHeadingIds, unified } from '@astrojs/markdown-remark';
import mdx from '@astrojs/mdx';
import react from '@astrojs/react';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig, fontProviders } from 'astro/config';
import rehypeAutolinkHeadings from 'rehype-autolink-headings';
import rehypeMermaid from 'rehype-mermaid';
import { codeBlocks } from './src/lib/code-blocks.mjs';

const LATIN =
  'U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD';
const LATIN_EXT =
  'U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+0304,U+0308,U+0329,U+1D00-1DBF,U+1E00-1E9F,U+1EF2-1EFF,U+2020,U+20A0-20AB,U+20AD-20C0,U+2113,U+2C60-2C7F,U+A720-A7FF';

/**
 * A self-hosted variable face, Latin and Latin Extended only: the site is in
 * English. The files are fontsource's; the names are the ones `@rustrak/ui`'s
 * tokens ask for, so nothing else has to change. `fallback` is the generic
 * family Astro sizes a local font to while the file loads, so the text does
 * not reflow when it arrives.
 */
const font = (name, cssVariable, dir, file, fallback) => ({
  provider: fontProviders.local(),
  name,
  cssVariable,
  fallbacks: [fallback],
  options: {
    variants: [
      [file('latin'), LATIN],
      [file('latin-ext'), LATIN_EXT],
    ].map(([src, unicodeRange]) => ({
      weight: '100 900',
      style: 'normal',
      unicodeRange: [unicodeRange],
      src: [`./node_modules/@fontsource-variable/${dir}/files/${src}`],
    })),
  },
});

/**
 * Puts each diagram in a `<figure class="diagram">`, the frame that scrolls a
 * wide one (`prose.css`). An `<svg>` cannot scroll itself.
 */
const wrapDiagrams = () => (tree) => {
  const walk = (node) => {
    node.children?.forEach((child, i) => {
      if (child.tagName === 'svg' && child.properties?.ariaRoleDescription) {
        node.children[i] = {
          type: 'element',
          tagName: 'figure',
          properties: { className: ['diagram'] },
          children: [child],
        };
      } else walk(child);
    });
  };
  walk(tree);
};

// Static output only. React renders the product shots at build time; nothing
// ships to the browser unless a component asks for it with `client:*`.
export default defineConfig({
  site: 'https://rustrak.dev',
  integrations: [react(), mdx()],
  markdown: {
    // Code is coloured with the product's own code tokens (`--code-*`), read
    // through Shiki's `css-variables` theme and mapped in `prose.css`: the
    // same palette as the landing's snippets and the dashboard. Mermaid
    // blocks are left alone for the plugin below.
    syntaxHighlight: { type: 'shiki', excludeLangs: ['mermaid'] },
    shikiConfig: { theme: 'css-variables' },
    // `unified`, not the default Sätteri, for the two rehype plugins.
    processor: unified({
      rehypePlugins: [
        // Diagrams are drawn at build time, in the Chromium Playwright
        // installs, and land in the page as SVG: no Mermaid in the browser.
        // `useMaxWidth: false` keeps the size they were laid out at, so a
        // wide one scrolls in its frame instead of shrinking past legibility.
        [
          rehypeMermaid,
          {
            strategy: 'inline-svg',
            // The posts' own `classDef`s colour their nodes for the light
            // default theme.
            mermaidConfig: {
              theme: 'default',
              flowchart: { useMaxWidth: false },
            },
          },
        ],
        wrapDiagrams,
        codeBlocks,
        // A permalink after every heading, shown on hover (`prose.css`). The
        // ids come first, from Astro's own slugger, so the links and the
        // contents beside the post agree.
        rehypeHeadingIds,
        [
          rehypeAutolinkHeadings,
          {
            behavior: 'append',
            properties: {
              // Kept out of the search index, or every heading reads `Title#`.
              'data-pagefind-ignore': '',
              class: 'heading-anchor',
              ariaHidden: true,
              tabIndex: -1,
            },
            content: { type: 'text', value: '#' },
          },
        ],
      ],
    }),
  },
  fonts: [
    // Inter with its optical-size axis, as the dashboard draws it.
    font(
      'Inter Variable',
      '--font-inter',
      'inter',
      (s) => `inter-${s}-opsz-normal.woff2`,
      'sans-serif',
    ),
    font(
      'Geist Mono Variable',
      '--font-geist-mono',
      'geist-mono',
      (s) => `geist-mono-${s}-wght-normal.woff2`,
      'monospace',
    ),
  ],
  vite: {
    plugins: [tailwindcss()],
    // Icons stay files: inlined, each would sit in the HTML and again in the
    // script that swaps the flow's tiles.
    build: { assetsInlineLimit: 0 },
    // The demo player imports its own CSS from its package, which Node cannot
    // load while prerendering: bundled, Vite handles it.
    resolve: {
      noExternal: [
        '@rustrak/openshowcase-player-react',
        '@rustrak/openshowcase-player-core',
      ],
    },
  },
});
