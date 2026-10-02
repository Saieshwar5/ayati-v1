import { defineConfig } from '@playwright/test';
import { join } from 'node:path';

const run = process.env.AYATI_TEST_RUN_DIR;
if (!run) throw new Error('Use npm run test:e2e (or test:live) to create an isolated run and reports.');
const live = process.env.AYATI_TEST_MODE === 'live';

export default defineConfig({
  testDir: './e2e',
  testMatch: live ? 'live-agent.spec.ts' : 'conversation.spec.ts',
  outputDir: join(run, 'results'),
  workers: 1,
  fullyParallel: false,
  forbidOnly: true,
  retries: 0,
  failOnFlakyTests: true,
  timeout: live ? 180_000 : 30_000,
  expect: { timeout: live ? 120_000 : 5_000 },
  reporter: [
    ['list'],
    ['html', { outputFolder: join(run, 'html'), open: 'never' }],
    ['json', { outputFile: join(run, 'playwright.json') }],
    ['./e2e/reporter.ts'],
  ],
  use: {
    browserName: 'chromium',
    viewport: { width: 1366, height: 768 },
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    video: 'off',
  },
});
