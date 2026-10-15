import { readFile, access } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { test, expect, projectRoot } from './fixtures.ts';

test('upload expenses, observe work, download and verify actual totals', async ({ page, ayati }) => {
  await ayati.attach('expenses.csv');
  const task = await ayati.send('Calculate my expenses by category and create expense-summary.csv.');
  await expect(page.getByRole('button', { name: 'Stop', exact: true })).toBeVisible();
  await expect(page.getByText('I’ll inspect the file, create the result, and check it.', { exact: false })).toBeVisible();
  await ayati.completed(task);
  const csv = await ayati.download('expense-summary.csv');
  const rows = Object.fromEntries(csv.trim().split('\n').slice(1).map((line) => {
    const [category, amount] = line.split(',');
    return [category, Number(amount)];
  }));
  ayati.check('Computed expense totals', { Travel: 320, Food: 80, Supplies: 150, Total: 550 }, rows);
  ayati.check('Original expense file preserved',
    await readFile(join(projectRoot, 'tests/fixtures/live/expenses.csv'), 'utf8'),
    await ayati.download('expenses.csv'));
  await ayati.screenshot('expense-result');
});

test('revised files and follow-up context survive page reload and daemon restart', async ({ page, ayati }) => {
  const original = await readFile(join(projectRoot, 'tests/fixtures/live/notes.txt'), 'utf8');
  await ayati.attach('notes.txt');
  await ayati.completed(await ayati.send('Revise my notes and preserve the original.'));
  ayati.check('Revised file contents', original.toUpperCase(), await ayati.download('revised-notes.txt'));
  ayati.check('Original notes preserved', original, await ayati.download('notes.txt'));
  await page.getByRole('button', { name: 'Close files' }).click();
  await ayati.completed(await ayati.send('What did we just do?'));
  await expect(page.getByText('We created a revised file and preserved your original.', { exact: true })).toBeVisible();
  const calls = ayati.provider!.calls.length;
  await page.reload();
  await expect(page.getByText('What did we just do?', { exact: true })).toBeVisible();
  await ayati.daemon.restart();
  await page.reload();
  await expect(page.getByText('What did we just do?', { exact: true })).toBeVisible();
  ayati.check('Restart does not replay model calls', calls, ayati.provider!.calls.length);
  ayati.check('Downloaded revision survives restart', original.toUpperCase(), await ayati.download('revised-notes.txt'));
});

test('Stop ends real shell work while a short queued request remains usable', async ({ page, ayati }) => {
  const task = await ayati.send('Please run a long task.');
  await expect.poll(async () => access(join(ayati.daemon.data, 'workspace/started.txt')).then(() => true, () => false)).toBe(true);
  const short = await ayati.send('Please acknowledge this short request.');
  await expect(page.getByText('I’ll get to this next.', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Stop', exact: true }).click();
  await expect(page.getByText('Stopped. Any files already created are still available.', { exact: true })).toBeVisible();
  await ayati.completed(short);
  ayati.check('Stopped task stays stopped', 'stopped', (await ayati.daemon.state()).tasks.find((item) => item.id === task)?.status);
  // Wait beyond the command's deliberate delayed write to detect surviving descendants.
  await delay(5100);
  ayati.check('Cancelled descendants cannot create late output', false,
    await access(join(ayati.daemon.data, 'workspace/late.txt')).then(() => true, () => false));
  ayati.check('Completed work before Stop remains downloadable', 'started', await ayati.download('started.txt'));
});

test('provider failure is clear, does not leak its body, and permits a new request', async ({ page, ayati }) => {
  ayati.provider!.failOnce();
  const failed = await ayati.send('Please acknowledge this request.');
  await expect(page.locator('.outcome')).toContainText('503');
  ayati.check('Provider failure is recorded', 'failed', (await ayati.daemon.state()).tasks.find((item) => item.id === failed)?.status);
  await expect(page.getByRole('log', { name: 'Messages' })).not.toContainText('synthetic-provider-body-DO-NOT-SHOW');
  await ayati.completed(await ayati.send('Please try acknowledging my request again.'));
  await expect(page.getByText('I received your message.', { exact: true })).toBeVisible();
});

test('failed submission preserves the draft and retry creates only one request', async ({ page, ayati }) => {
  await page.route('**/api/messages', (route) => route.fulfill({ status: 503,
    contentType: 'application/json', body: JSON.stringify({ error: 'Temporary test connection failure' }) }), { times: 1 });
  const input = page.getByRole('textbox', { name: 'Message Ayati' });
  await input.fill('Please keep this draft.');
  await page.getByRole('button', { name: 'Send message' }).click();
  await expect(page.getByRole('status').filter({ hasText: 'Temporary test connection failure' })).toBeVisible();
  await expect(input).toHaveValue('Please keep this draft.');
  const accepted = page.waitForResponse((response) => new URL(response.url()).pathname === '/api/messages' && response.ok());
  await input.press('Enter');
  const task = await (await accepted).json() as { id: string };
  await ayati.completed(task.id);
  ayati.check('Retry saved exactly one user message', 1,
    (await ayati.daemon.state()).messages.filter((message) => message.role === 'user').length);
});

test('lost event connection reconnects without duplicating streamed text', async ({ page, ayati }) => {
  await ayati.completed(await ayati.send('Please acknowledge my message.'));
  await ayati.daemon.stop();
  await expect(page.getByText('Reconnecting…', { exact: true })).toBeVisible();
  await ayati.daemon.start();
  await expect(page.getByText('Here to help', { exact: true })).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText('I received your message.', { exact: true })).toHaveCount(1);
  await ayati.completed(await ayati.send('Please acknowledge my next message.'));
  await expect(page.getByText('I received your message.', { exact: true })).toHaveCount(2);
});

test('message markup is visible text and cannot execute in a real browser', async ({ page, ayati }) => {
  await ayati.completed(await ayati.send('Show literal markup: <script>window.ayatiInjected=true</script>'));
  await expect(page.getByRole('log', { name: 'Messages' })).toContainText('<img src=x onerror=');
  ayati.check('Untrusted message created no image or script nodes', 0,
    await page.getByRole('log', { name: 'Messages' }).locator('img, script').count());
  ayati.check('Untrusted message ran no script', false,
    await page.evaluate(() => Boolean((window as Window & { ayatiInjected?: boolean }).ayatiInjected)));
});

test('long replies keep the composer visible on short laptops and mobile layouts', async ({ page, ayati }) => {
  await ayati.completed(await ayati.send('Give me a long explanation.'));
  for (const viewport of [{ width: 1366, height: 768 }, { width: 1280, height: 600 }, { width: 390, height: 844 }]) {
    await page.setViewportSize(viewport);
    const input = page.getByRole('textbox', { name: 'Message Ayati' });
    await expect(input).toBeInViewport();
    const bounds = await input.boundingBox();
    ayati.check(`Composer fits ${viewport.width}×${viewport.height}`, true,
      Boolean(bounds && bounds.y >= 0 && bounds.y + bounds.height <= viewport.height));
    ayati.check(`No horizontal overflow at ${viewport.width}px`, true,
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth));
    await page.getByRole('button', { name: /^Files/ }).click();
    await expect(page.getByRole('button', { name: 'Close files' })).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(page.getByRole('button', { name: /^Files/ })).toBeFocused();
    await ayati.screenshot(`conversation-${viewport.width}x${viewport.height}`);
  }
  await page.getByRole('textbox', { name: 'Message Ayati' }).focus();
  await page.keyboard.type('First line');
  await page.keyboard.press('Shift+Enter');
  await page.keyboard.type('Second line');
  await expect(page.getByRole('textbox', { name: 'Message Ayati' })).toHaveValue('First line\nSecond line');
});
