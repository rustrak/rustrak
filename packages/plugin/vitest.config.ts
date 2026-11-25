import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  resolve: {
    // A plugin's rustrak.config.ts imports the package by name; in tests that
    // is the source, not a build.
    alias: {
      '@rustrak/plugin': fileURLToPath(
        new URL('./src/index.ts', import.meta.url),
      ),
    },
  },
  test: {
    environment: 'node',
    // Building a plugin runs cargo and the rustrak binary.
    testTimeout: 300_000,
  },
});
