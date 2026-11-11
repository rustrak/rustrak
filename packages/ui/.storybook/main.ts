import type { StorybookConfig } from '@storybook/react-vite';
import tailwindcss from '@tailwindcss/vite';
import { composite, parseColor } from '../src/styles/color.ts';
import { dark, resolve } from '../src/styles/token-values.ts';

/**
 * A dark-theme colour token as the hex Storybook's manager takes. A
 * translucent one is laid over `over` first, as the browser would.
 */
function hex(token: string, over = '--surface-canvas'): string {
  let rgb = parseColor(resolve(token, dark));
  if (rgb.a < 1) rgb = composite(rgb, parseColor(resolve(over, dark)));
  return `#${[rgb.r, rgb.g, rgb.b]
    .map((c) =>
      Math.round(c * 255)
        .toString(16)
        .padStart(2, '0'),
    )
    .join('')}`;
}

const px = (token: string) => Number.parseFloat(resolve(token, dark));

/**
 * The manager is a separate bundle that never loads `tokens.css`, so the
 * values `theme.ts` and `manager-head.html` use are resolved here, in Node,
 * and handed over in the manager's head.
 */
const managerTokens = {
  brand: hex('--surface-brand'),
  canvas: hex('--surface-canvas'),
  panel: hex('--surface-panel'),
  surface: hex('--surface'),
  hover: hex('--surface-hover'),
  border: hex('--border', '--surface-panel'),
  fg: hex('--fg'),
  fgSecondary: hex('--fg-secondary'),
  fgSubtle: hex('--fg-subtle'),
  fgInverse: hex('--fg-inverse'),
  radiusLg: px('--radius-lg'),
  radiusMd: px('--radius-md'),
};
const tokensScript = `<script>window.RUSTRAK_TOKENS = ${JSON.stringify(managerTokens)};</script>`;

const config: StorybookConfig = {
  stories: ['../src/**/*.mdx', '../src/**/*.stories.tsx'],
  addons: [
    '@storybook/addon-docs',
    '@storybook/addon-a11y',
    '@storybook/addon-themes',
    '@storybook/addon-vitest',
  ],
  framework: {
    name: '@storybook/react-vite',
    options: {},
  },
  // Where the manager's brand mark is served from. It is a file rather than a
  // data URI so the wordmark stays the artwork and not a string nobody can
  // read; `public/rustrak-wordmark.svg` is a hand copy of the paths in
  // `components/brand/wordmark.tsx`.
  staticDirs: ['./public'],
  // The preview needs them too: the docs pages wear the same theme.
  previewHead: (head) => `${head}${tokensScript}`,
  managerHead: (head) => `${head}${tokensScript}
<style>:root {
  --rk-meta: ${hex('--fg-meta', '--surface-canvas')};
  --rk-muted: ${hex('--fg-muted', '--surface-panel')};
  --rk-chip: ${hex('--surface-chip', '--surface-panel')};
}</style>`,
  // Without this the stories render unstyled: Tailwind is what compiles the
  // tokens and the utilities, and the Storybook server does not ship it.
  viteFinal: async (config) => {
    config.plugins = [...(config.plugins ?? []), tailwindcss()];
    return config;
  },
  typescript: {
    // Controls and the props table come from the real types, not from a
    // hand-written list that goes stale.
    reactDocgen: 'react-docgen-typescript',
    reactDocgenTypescriptOptions: {
      shouldExtractLiteralValuesFromEnum: true,
      shouldRemoveUndefinedFromOptional: true,
      propFilter: (prop) => !prop.parent?.fileName.includes('node_modules'),
      /*
       * The docgen plugin's default is to read every `.tsx` in the project and
       * skip only `*.stories.tsx`, so it picks up `.storybook/preview.tsx` and
       * warns on every boot that the file is not in the TypeScript project.
       *
       * The warning is not worth chasing through tsconfig: there are no
       * components in `.storybook`. It is configuration, there is nothing in it
       * to document, and parsing it for props was wasted work before it was a
       * warning. The default is restated because giving `exclude` at all
       * replaces it.
       */
      exclude: ['**/*.stories.tsx', '.storybook/**'],
    },
  },
};

export default config;
