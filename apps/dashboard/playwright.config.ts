import { defineConfig, devices } from '@playwright/test';

/**
 * End-to-end tests: the compiled dashboard, served by the real Rust server on
 * a fresh SQLite database (`e2e/start-server.sh`). Nothing is mocked, so a
 * test fails when the page, the API or the contract between them breaks.
 *
 * Read `e2e/README.md` before adding a test: it says what earns one.
 */
const port = Number(process.env.E2E_PORT ?? 8790);
export const baseURL = `http://127.0.0.1:${port}`;
const CI = Boolean(process.env.CI);

export default defineConfig({
  testDir: './e2e',
  // Writes the coverage report when `E2E_COVERAGE` is set; a no-op otherwise.
  globalTeardown: './e2e/coverage.ts',
  fullyParallel: true,
  forbidOnly: CI,
  // One retry in CI, so a flake shows up as "flaky" in the report rather than
  // as a red build or, worse, as silence.
  retries: CI ? 1 : 0,
  reporter: CI ? [['github'], ['html', { open: 'never' }]] : 'list',
  timeout: 30_000,
  expect: { timeout: 10_000 },
  use: {
    baseURL,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    { name: 'setup', testMatch: /auth\.setup\.ts/ },
    {
      name: 'chromium',
      use: {
        ...devices['Desktop Chrome'],
        storageState: 'e2e/.auth/admin.json',
      },
      dependencies: ['setup'],
    },
  ],
  webServer: {
    command: 'bash e2e/start-server.sh',
    url: `${baseURL}/health`,
    env: { E2E_PORT: String(port) },
    // A cold `cargo run` compiles the server first.
    timeout: 10 * 60_000,
    reuseExistingServer: !CI,
    stdout: 'ignore',
    stderr: 'pipe',
  },
});
