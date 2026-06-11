import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: 'tests/e2e',
  fullyParallel: true,
  retries: process.env.CI ? 2 : 0,
  reporter: [['list']],
  use: {
    baseURL: 'http://localhost:5173',
    trace: 'on-first-retry',
    // The card orchestrator honors prefers-reduced-motion by skipping
    // animation flights; gameplay tests assert on state, not motion, and the
    // full animation chain adds ~1.7s per trick. The animation-specific test
    // opts back in with test.use({ reducedMotion: 'no-preference' }).
    contextOptions: { reducedMotion: 'reduce' },
  },
  webServer: [
    {
      command: 'make -C .. backend DB=',
      url: 'http://localhost:3000/health',
      reuseExistingServer: !process.env.CI,
      timeout: 120_000,
    },
    {
      command: 'pnpm dev',
      url: 'http://localhost:5173',
      reuseExistingServer: !process.env.CI,
      timeout: 30_000,
    },
  ],
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
});
