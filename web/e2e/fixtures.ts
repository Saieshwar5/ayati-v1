import { test as base, expect, type TestInfo } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { TestDaemon, projectRoot } from './daemon.ts';
import { Evidence } from './evidence.ts';
import { startScriptedModel } from './scripted-model.ts';

type Provider = Awaited<ReturnType<typeof startScriptedModel>>;
type Harness = {
  daemon: TestDaemon;
  provider?: Provider;
  evidence: Evidence;
  send: (text: string) => Promise<string>;
  attach: (name: string) => Promise<void>;
  download: (name: string) => Promise<string>;
  completed: (task: string) => Promise<void>;
  check: (name: string, expected: unknown, actual: unknown) => void;
  screenshot: (name: string) => Promise<void>;
};

export const test = base.extend<{ ayati: Harness }>({
  ayati: async ({ page }, use, info) => {
    const live = process.env.AYATI_TEST_MODE === 'live';
    const provider = live ? undefined : await startScriptedModel();
    const daemon = new TestDaemon(info.outputPath('ayati-data'), provider?.url);
    const evidence = new Evidence();
    evidence.observe(page);
    const check: Harness['check'] = (name, expected, actual) => {
      evidence.check(name, expected, actual);
      expect(actual, name).toEqual(expected);
    };
    try {
      await daemon.start();
      await page.goto(daemon.base);
      await expect(page.getByRole('button', { name: 'Send message' })).toBeEnabled();
      await use({
        daemon, provider, evidence, check,
        send: async (text) => {
          const accepted = page.waitForResponse((response) =>
            new URL(response.url()).pathname === '/api/messages' && response.request().method() === 'POST');
          await page.getByRole('textbox', { name: 'Message Ayati' }).fill(text);
          await page.getByRole('button', { name: 'Send message' }).click();
          const response = await accepted;
          expect(response.ok(), 'Message accepted by the real daemon').toBe(true);
          const task = await response.json() as { id: string };
          evidence.record('user_request', { task_id: task.id, text });
          return task.id;
        },
        attach: async (name) => {
          const chooser = page.waitForEvent('filechooser');
          await page.getByRole('button', { name: 'Attach a file' }).click();
          await (await chooser).setFiles(join(projectRoot, 'tests/fixtures/live', name));
          await expect(page.getByRole('button', { name: `${name} ×`, exact: true })).toBeVisible();
          evidence.record('attachment', { name });
        },
        download: async (name) => {
          const panel = page.getByRole('complementary', { name: 'Your files' });
          if (!await panel.isVisible()) await page.getByRole('button', { name: /^Files/ }).click();
          const link = panel.getByRole('link').filter({ has: page.getByText(name, { exact: true }) });
          await expect(link).toHaveCount(1);
          const pending = page.waitForEvent('download');
          await link.click();
          const download = await pending;
          const path = info.outputPath(`download-${name}`);
          await download.saveAs(path);
          expect(await download.failure()).toBeNull();
          evidence.record('download', { name, bytes_verified_at: path });
          await info.attach(name, { path, contentType: 'application/octet-stream' });
          return readFile(path, 'utf8');
        },
        completed: async (task) => {
          await expect.poll(async () => (await daemon.state()).tasks.find((item) => item.id === task)?.status,
            { timeout: live ? 120_000 : 5_000 }).not.toMatch(/^(queued|running)$/);
          const outcome = (await daemon.state()).tasks.find((item) => item.id === task);
          check('Task completed successfully', 'completed', outcome?.status);
        },
        screenshot: async (name) => {
          await screenshot(page, info, name);
        },
      });
      evidence.check('No unhandled browser errors', [], evidence.browserErrors);
      expect.soft(evidence.browserErrors, 'No unhandled browser errors').toEqual([]);
    } catch (error) {
      evidence.record('scenario_error', String(error));
      throw error;
    } finally {
      try { await daemon.stop(); }
      catch (error) { evidence.record('cleanup_error', String(error)); throw error; }
      finally {
        try { await evidence.save(info, daemon, provider?.calls ?? []); }
        finally { await provider?.close(); }
      }
    }
  },
});

async function screenshot(page: import('@playwright/test').Page, info: TestInfo, name: string): Promise<void> {
  const path = info.outputPath(`${name}.png`);
  await page.screenshot({ path });
  await info.attach(name, { path, contentType: 'image/png' });
}

export { expect, projectRoot };
