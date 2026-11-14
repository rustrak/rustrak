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
  // `"use client"` at the top of every module. The whole package is client
  // code, so the consuming framework does not have to resolve it per module.
  outputOptions: { banner: "'use client';" },
  // React, Base UI and the icon library are resolved by the application.
  // Bundling them would duplicate React's context tree and break every hook.
  deps: { neverBundle: [/^react/, /^react-dom/, /^@base-ui\/react/] },
});
