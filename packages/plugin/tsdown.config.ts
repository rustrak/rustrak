import { defineConfig } from 'tsdown';

export default defineConfig({
  entry: ['src/index.ts', 'src/cli.ts', 'src/ui/index.tsx'],
  format: ['esm'],
  dts: true,
  platform: 'node',
});
