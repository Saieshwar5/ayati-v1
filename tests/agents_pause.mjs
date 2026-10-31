// Pause all agents with real Goose: an admin's emergency switch stops a request
// that is genuinely running, refuses new work with a reason, reaches every open
// tab through the live stream, and resumes cleanly.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until, root } from './fixtures/polling.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');

test('Pause all agents stops running Goose work at once, tells every tab and resumes cleanly', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 240000,
}, async t => {
  let browser; const errors = [];
  t.after(async () => { await browser?.close(); });
  const app = await workerFixture(t, { mode: 'pooled', control: true, capacity: 1, workspaceLimit: 1, modelLimit: 1, leaseSeconds: 15 });
  const w = app.path.split('/').at(-1);
  await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const attempt = reply => app.sql(`SELECT a.state FROM relay_worker_assignments a JOIN relay_chat_jobs j ON a.id=j.claim WHERE j.reply='${reply}'`);
  const reply = id => app.sql(`SELECT state||'|'||content FROM relay_chat_messages WHERE id='${id}'`);

  const runningText = 'Draft and send 400 customer emails';
  app.provider.setHeldMarkers([runningText]);
  const running = await app.send(runningText);
  await until(() => attempt(running.reply) === 'running', 'The request did not start', 35000);
  const queued = await app.send('Then call every supplier');

  browser = await chromium.launch({ headless: true });
  async function pageFor(url) {
    const context = await browser.newContext({ viewport: { width: 1440, height: 960 } });
    const [name, value] = app.cookie.split('=');
    await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
    const page = await context.newPage(); page.on('pageerror', error => errors.push(error.message));
    await page.goto(url); return page;
  }
  // A second tab is already open on Home; it should learn about the pause live.
  const home = await pageFor(`${app.base}/w/${w}/?view=home`);
  const now = await pageFor(`${app.base}/w/${w}/?view=activity&tab=now`);
  await now.getByRole('button', { name: 'Pause all agents', exact: true }).click();
  const dialog = now.getByRole('dialog', { name: 'Pause all agents?', exact: true });
  await dialog.getByText(/2 requests that are running or waiting will be stopped/).waitFor();
  await dialog.getByLabel(/Reason/).fill('Wrong emails going out');
  await dialog.getByRole('button', { name: 'Pause all agents', exact: true }).click();
  await now.locator('.agents-paused-banner').getByText(/Wrong emails going out/).waitFor();
  await now.getByRole('button', { name: 'Resume agents', exact: true }).first().waitFor();

  // The running attempt is cancelled and both requests say why.
  await until(() => attempt(running.reply) === 'finished', 'The running attempt was not stopped', 20000);
  for (const job of [running, queued]) assert.equal(reply(job.reply), 'cancelled|Stopped because all agents were paused.');
  // Releasing the model afterwards can't bring the reply back.
  app.provider.setHeldMarkers([]);
  await new Promise(resolve => setTimeout(resolve, 3000));
  assert.equal(reply(running.reply), 'cancelled|Stopped because all agents were paused.');
  app.pass('An admin paused from Activity → Now; the running real-Goose attempt and the queued request stopped with a reason, and a late model reply did not revive them');

  const refused = await app.request(`${app.path}/chats/${app.conversation}/messages`, app.cookie,
    { id: randomUUID(), content: 'One more thing', agent: app.profile.id }, 409);
  assert.match(refused.value.error, /All agents in this workspace are paused/);
  await home.locator('.agents-paused-banner').getByText(/Wrong emails going out/).waitFor({ timeout: 15000 });
  await home.screenshot({ path: join(app.data, 'agents-paused-home.png') });
  app.pass('New agent work is refused with a reason (409), and an already open tab showed the pause through the live stream');

  await home.locator('.agents-paused-banner').getByRole('button', { name: 'Resume agents', exact: true }).click();
  await home.locator('.agents-paused-banner').waitFor({ state: 'detached' });
  await now.locator('.agents-paused-banner').waitFor({ state: 'detached', timeout: 15000 });
  const after = await app.send('Summarise today in one line');
  await app.done(after);
  assert.equal(reply(queued.reply).split('|')[0], 'cancelled', 'stopped work does not restart on resume');
  const log = (await app.request(`${app.path}/activity?kind=all`, app.cookie)).value.activity.map(entry => entry.sentence);
  assert.ok(log.includes('paused all agents and stopped 2 requests: Wrong emails going out'), JSON.stringify(log));
  assert.ok(log.includes('resumed all agents'));
  assert.deepEqual(errors, []);
  app.pass('Resuming from the banner cleared it in every tab; the executor took new work and completed it; stopped work stayed stopped; the audit log names both actions');
});
