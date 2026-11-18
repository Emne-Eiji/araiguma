import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: 'e2e',
  timeout: 30_000,
  expect: { timeout: 10_000 },
  fullyParallel: true,
  reporter: [['list']],
  use: {
    baseURL: 'http://127.0.0.1:5199',
    trace: 'off',
  },
  webServer: {
    // Expects `npm run build` to have run (scripts/test-all.sh does).
    command: 'npm run preview',
    port: 5199,
    reuseExistingServer: false,
    timeout: 60_000,
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
});
