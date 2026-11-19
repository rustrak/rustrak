import { defineConfig } from 'tsdown';

export default defineConfig({
  entry: ['src/index.ts'],
  format: ['esm'],
  platform: 'neutral',
  dts: true,
  sourcemap: true,
  clean: true,
  // One output file per source module, not one bundle. A bundler cannot split
  // a module, so a single `index.js` put every component (and recharts, the
  // table, every Base UI part) in whichever chunk imported any of them: a
  // consumer's first page paid for the whole design system. Kept as modules,
  // each page's chunk takes only the components it renders.
  unbundle: true,
  // `"use client"` at the top of every component module, so the consuming
  // framework does not have to resolve it per module. Not on the barrel: a
  // `'use client'` barrel is one client reference carrying every export, so a
  // Server Component importing only `Wordmark` shipped recharts, the table and
  // Base UI to the browser. Not on `lib/` either: `cn` and the tokens are
  // plain functions a Server Component should be able to call.
  outputOptions: {
    banner: (chunk) =>
      chunk.fileName.startsWith('components/') ? "'use client';" : '',
  },
  // React, Base UI and the icon library are resolved by the application.
  // Bundling them would duplicate React's context tree and break every hook.
  deps: { neverBundle: [/^react/, /^react-dom/, /^@base-ui\/react/] },
});
