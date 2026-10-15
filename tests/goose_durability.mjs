import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { setup, until } from './fixtures/goose_app.mjs';

test('duplicate chat delivery returns the saved job and changed content conflicts', { timeout: 60000 }, async (t) => {
  const app = await setup(t);
  const request_id = randomUUID();
  const [first, repeated] = await Promise.all([
    app.request('/messages', { content: 'hello', request_id }),
    app.request('/messages', { content: 'hello', request_id }),
  ]);
  assert.equal(first.id, repeated.id);
  assert.equal((await app.done(first.id)).task.status, 'completed');
  const conflict = await fetch(app.base + '/messages', {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ content: 'different', request_id }),
  });
  assert.equal(conflict.status, 409);
  await app.stop();
  await app.start();
  assert.equal((await app.request('/messages', { content: 'hello', request_id })).id, first.id);
  assert.equal((await app.state()).messages.filter((m) => m.role === 'user').length, 1);
  assert.equal(app.provider.requests.length, 1);
});

test('a host crash recovers queued input once and does not rerun claimed or stopped jobs', { timeout: 60000 }, async (t) => {
  const app = await setup(t);
  const active = await app.submit('stall');
  await until(() => app.provider.requests.length > 0, 'No active model request');
  const request_id = randomUUID();
  const waiting = await app.request('/messages', { content: 'continue queued work', request_id });
  const cancelled = await app.submit('cancelled queued work');
  await app.request(`/tasks/${cancelled.id}/stop`, {});
  const before = app.provider.requests.length;
  await app.crash();
  await app.start();
  assert.equal((await app.done(waiting.id)).task.status, 'completed');
  const state = await app.state();
  assert.equal(state.tasks.find((r) => r.id === active.id).status, 'interrupted');
  assert.equal(state.tasks.find((r) => r.id === cancelled.id).status, 'stopped');
  assert.equal(app.provider.requests.length, before + 1);
  assert.equal((await app.request('/messages', { content: 'continue queued work', request_id })).id, waiting.id);
  assert.equal(app.provider.requests.length, before + 1);
});

test('service shutdown parks queued work and invalidates an in-flight approval', { timeout: 60000 }, async (t) => {
  const app = await setup(t, true);
  const active = await app.submit('tool wait');
  const permission = await until(async () => (await app.state()).permissions?.[0], 'No pending approval');
  const waiting = await app.submit('continue queued after restart');
  await app.stop();
  const before = app.provider.requests.length;
  app.configure({ OPENAI_API_KEY: '' });
  await app.start();
  assert.equal((await app.request('/health')).model_ready, false);
  assert.equal((await app.state()).tasks.find((r) => r.id === waiting.id).status, 'queued');
  assert.equal(app.provider.requests.length, before);
  await app.stop();
  app.configure({ OPENAI_API_KEY: 'fixture-not-a-secret' });
  await app.start();
  assert.equal((await app.done(waiting.id)).task.status, 'completed');
  assert.equal((await app.state()).tasks.find((r) => r.id === active.id).status, 'interrupted');
  const response = await fetch(app.base + `/permissions/${permission.id}`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{"allow":true}',
  });
  assert.equal(response.status, 400);
  const { actions } = await app.request(`/tasks/${active.id}/actions`);
  assert.equal(actions[0].status, 'cancelled');
  assert.equal(actions[0].decision, 'cancelled');
});

test('lost HTTP acknowledgement and browser reload retry the same saved job', { timeout: 60000 }, async (t) => {
  const app = await setup(t);
  const { chromium } = createRequire(new URL('../web/package.json', import.meta.url))('@playwright/test');
  const browser = await chromium.launch();
  t.after(() => browser.close());
  const page = await browser.newPage();
  let loseReply = true;
  const ids = [];
  await page.route('**/api/messages', async (route) => {
    ids.push(route.request().postDataJSON().request_id);
    if (loseReply) { await route.fetch(); await route.abort('failed'); }
    else await route.continue();
  });
  await page.goto(app.base.replace('/api', ''));
  const input = page.getByRole('textbox', { name: 'Message Ayati' });
  await input.fill('hello');
  await page.getByRole('button', { name: 'Send message' }).click();
  await until(async () => (await app.state()).tasks[0]?.status === 'completed', 'First delivery was not processed');
  assert.equal(await input.inputValue(), 'hello');
  loseReply = false;
  await page.reload();
  assert.equal(await input.inputValue(), 'hello');
  await page.getByRole('button', { name: 'Send message' }).click();
  await until(async () => (await input.inputValue()) === '', 'Retry was not acknowledged');
  assert.equal(ids.length, 2);
  assert.ok(ids[0]);
  assert.equal(ids[0], ids[1]);
  assert.equal((await app.state()).tasks.length, 1);
  assert.equal(app.provider.requests.length, 1);
});
