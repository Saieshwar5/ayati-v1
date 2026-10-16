import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { DatabaseSync } from 'node:sqlite';
import { randomUUID } from 'node:crypto';
import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { setup, until, root } from './fixtures/goose_app.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

function localDate(ms) {
  const d = new Date(ms);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}T${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
}

test('agent schedules create, pause, resume, retain uncertain saves and show durable run results', { timeout: 90000 }, async t => {
  const app = await setup(t, false, { AYATI_BROWSER: '0', AYATI_MEMORY: '0' });
  const browser = await chromium.launch(); t.after(() => browser.close());
  const page = await browser.newPage({ viewport: { width: 1280, height: 960 } });
  const errors = []; page.on('pageerror', e => errors.push(e.message));
  const open = async () => {
    await page.goto(app.base.replace('/api', '/?agent=local-agent'));
    await page.locator('#details-button').click();
    await page.getByRole('button', { name: 'Manage schedules' }).click();
    await page.getByText('No schedules for this agent yet.', { exact: true }).waitFor().catch(() => {});
  };
  await open();
  const dialog = page.locator('#agent-schedules');
  await dialog.getByText('Schedules are up to date.', { exact: true }).waitFor();
  assert.match(await dialog.locator('#schedules-agent').textContent(), /Ayati/);
  await page.locator('#schedule-form [name=content]').fill('Scheduled UI <script>must stay text</script>');
  await page.locator('#schedule-form [name=start]').fill(localDate(Date.now() - 3600000));
  await dialog.getByRole('button', { name: 'Create schedule', exact: true }).click();
  await dialog.getByText('Choose a valid future local time.', { exact: false }).waitFor();
  assert.equal((await app.request('/schedules')).schedules.length, 0);
  await page.locator('#schedule-form [name=start]').fill(localDate(Date.now() + 3600000));
  await page.locator('#schedule-form [name=repeat]').selectOption('interval');
  await page.locator('#schedule-form [name=amount]').fill('2');
  await page.locator('#schedule-form [name=unit]').selectOption('3600');
  // Lose the acknowledgement after the server commits. Retry, including after reload, must reuse its ID.
  let lost = true;
  await page.route('**/api/schedules', async route => {
    if (route.request().method() === 'POST' && lost) { lost = false; await route.fetch(); await route.abort('failed'); }
    else await route.continue();
  });
  await dialog.getByRole('button', { name: 'Create schedule', exact: true }).click();
  await dialog.getByRole('button', { name: 'Retry same save', exact: true }).waitFor();
  assert.equal((await app.request('/schedules')).schedules.length, 1);
  await page.reload();
  await page.locator('#details-button').click();
  await page.getByRole('button', { name: 'Manage schedules' }).click();
  await dialog.getByRole('button', { name: 'Retry same save', exact: true }).click();
  await dialog.getByText('Schedule saved.', { exact: true }).waitFor();
  let data = await app.request('/schedules');
  assert.equal(data.schedules.length, 1); assert.equal(data.schedules[0].interval_seconds, 7200);
  const id = data.schedules[0].id;
  const card = dialog.locator(`[data-schedule="${id}"]`);
  assert.equal(await card.locator('script').count(), 0);
  await card.getByRole('button', { name: 'Pause', exact: true }).click();
  await until(async () => (await app.request('/schedules')).schedules[0].state === 'paused', 'Pause failed');
  await card.getByRole('button', { name: 'Resume', exact: true }).click();
  await until(async () => (await app.request('/schedules')).schedules[0].state === 'active', 'Resume failed');
  await card.getByRole('button', { name: 'View runs' }).click();
  await dialog.getByText('No accepted runs yet.', { exact: true }).waitFor();
  await card.getByRole('button', { name: 'Pause', exact: true }).click();
  await page.locator('#schedule-form [name=content]').fill('Future one-time task');
  await page.locator('#schedule-form [name=start]').fill(localDate(Date.now() + 7200000));
  await page.locator('#schedule-form [name=repeat]').selectOption('interval');
  await page.locator('#schedule-form [name=amount]').fill('0');
  await page.locator('#schedule-form [name=repeat]').selectOption('once');
  await dialog.getByRole('button', { name: 'Create schedule', exact: true }).click();
  await until(async () => (await app.request('/schedules')).schedules.find(s => s.content === 'Future one-time task' && s.interval_seconds === null), 'One-time form did not save');
  // Existing developer API can create an immediately due one-time run; the UI requires future times.
  const once = await app.request('/schedules', { id: randomUUID(), content: 'SCHEDULE_RESULT_MARKER', run_at_ms: Date.now(), interval_seconds: null });
  const run = await until(async () => (await app.request(`/schedules/${once.id}/runs`)).runs.find(r => r.task.status === 'completed'), 'One-time schedule did not finish');
  await dialog.locator('#schedules-refresh').click();
  const onceCard = dialog.locator(`[data-schedule="${once.id}"]`);
  await onceCard.getByText('Completed · occurrence accepted', { exact: false }).waitFor();
  assert.equal(await onceCard.getByRole('button', { name: 'Resume', exact: true }).count(), 0);
  await onceCard.getByRole('button', { name: 'View runs' }).click();
  await dialog.locator('#schedule-runs').getByRole('button').first().click();
  await dialog.getByText('Run outcome: completed', { exact: true }).waitFor();
  assert.match(await dialog.locator('#schedule-result').textContent(), /SCHEDULE_RESULT_MARKER/);
  const raw = (path, init) => fetch(app.base + path, init);
  assert.equal((await raw('/schedules/missing/runs')).status, 400);
  assert.equal((await raw(`/schedules/${once.id}/enabled`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ enabled: true }) })).status, 400);
  const db = new DatabaseSync(join(app.data, 'ayati.sqlite'));
  db.prepare('UPDATE dispatch_control SET paused=1 WHERE id=1').run(); db.close();
  await dialog.locator('#schedules-refresh').click();
  await dialog.locator('#schedules-dispatch').waitFor();
  await page.setViewportSize({ width: 390, height: 844 });
  await mkdir(join(root, '.test-runs/schedules'), { recursive: true });
  assert.ok(await dialog.evaluate(el => el.scrollWidth <= el.clientWidth + 1));
  await dialog.evaluate(el => { el.scrollTop = 0; });
  await page.screenshot({ path: join(root, '.test-runs/schedules/mobile.png') });
  // A stale list cannot issue actions after a failed refresh; creation is disabled too.
  await page.route('**/api/schedules', route => route.abort('failed'));
  await dialog.locator('#schedules-refresh').click();
  await dialog.getByText('Controls are disabled until reconnected.', { exact: false }).waitFor();
  assert.equal(await card.getByRole('button', { name: 'Resume', exact: true }).isDisabled(), true);
  assert.equal(await dialog.getByRole('button', { name: 'Create schedule', exact: true }).isDisabled(), true);
  await page.unroute('**/api/schedules');
  await app.stop(); await app.start();
  data = await app.request('/schedules');
  assert.equal(data.dispatch_paused, true);
  assert.equal(data.schedules.find(s => s.id === id).state, 'paused');
  assert.equal((await app.request(`/schedules/${once.id}/runs`)).runs[0].task.id, run.task.id);
  assert.equal((await app.request(`/schedules/${once.id}/runs`)).runs.length, 1);
  assert.deepEqual(errors, []);
});
