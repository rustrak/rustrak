import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

// End to end: builds real plugins with cargo, as an author's machine does.
export default defineConfig({
  resolve: {
    alias: {
      '@rustrak/plugin': fileURLToPath(
        new URL('./src/index.ts', import.meta.url),
      ),
    },
  },
  test: {
    environment: 'node',
    include: ['tests/**/*.e2e.ts'],
    testTimeout: 300_000,
  },
});
