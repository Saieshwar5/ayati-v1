import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { setup, until } from './fixtures/goose_app.mjs';

test('local event receipts survive an unconfigured crash and duplicate delivery creates one Goose run', { timeout: 90000 }, async (t) => {
  const app = await setup(t, false, { OPENAI_API_KEY: '' });
  const event = { source: 'fixture.shop', event_id: 'order-1', content: 'Summarize synthetic order' };
  const [first, duplicate] = await Promise.all([
    app.request('/intake/events', event), app.request('/intake/events', event),
  ]);
  assert.equal(first.id, duplicate.id);
  assert.equal(first.status, 'queued');
  assert.equal(app.provider.requests.length, 0);
  for (const [body, headers, expected] of [
    [{ ...event, content: 'Conflicting event' }, {}, 409],
    [event, { Origin: 'https://untrusted.example' }, 403],
  ]) {
    const response = await fetch(app.base + '/intake/events', {
      method: 'POST', headers: { 'Content-Type': 'application/json', ...headers }, body: JSON.stringify(body),
    });
    assert.equal(response.status, expected);
  }
  await app.crash();
  app.configure({ OPENAI_API_KEY: 'fixture-not-a-secret' });
  await app.start();
  assert.equal((await app.done(first.id)).task.status, 'completed');
  assert.equal((await app.request('/intake/events', event)).id, first.id);
  assert.equal(app.provider.requests.length, 1);
});

test('a saved one-time schedule survives service restart and delivers once', { timeout: 90000 }, async (t) => {
  const app = await setup(t);
  const schedule = { id: randomUUID(), content: 'Morning synthetic shop summary', run_at_ms: Date.now() + 1500 };
  await app.request('/schedules', schedule);
  await app.request('/schedules', schedule);
  const conflict = await fetch(app.base + '/schedules', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ ...schedule, content: 'Changed' }),
  });
  assert.equal(conflict.status, 409);
  await app.stop();
  await app.start();
  const delivered = await until(async () => {
    const { schedules } = await app.request('/schedules');
    return schedules[0]?.last_task_id && schedules[0];
  }, 'Saved schedule did not deliver');
  assert.equal(delivered.state, 'completed'); // Occurrence accepted, not a claim about its outcome.
  assert.equal((await app.done(delivered.last_task_id)).task.status, 'completed');
  await app.stop();
  await app.start();
  assert.equal((await app.request('/schedules')).schedules[0].last_task_id, delivered.last_task_id);
  assert.equal((await app.state()).tasks.length, 1);
  assert.equal(app.provider.requests.length, 1);
});

test('an overdue interval coalesces behind active work and pausing cancels its queued delivery', { timeout: 90000 }, async (t) => {
  const app = await setup(t);
  const active = await app.submit('stall');
  await until(() => app.provider.requests.length === 1, 'No active Goose call');
  const schedule = { id: randomUUID(), content: 'Scheduled synthetic shop summary', run_at_ms: Date.now() - 3600000, interval_seconds: 60 };
  await app.request('/schedules', schedule);
  const delivered = await until(async () => (await app.request('/schedules')).schedules[0]?.last_task_id, 'No queued occurrence');
  assert.equal((await app.state()).tasks.length, 2);
  await app.request(`/schedules/${schedule.id}/enabled`, { enabled: false });
  assert.equal((await app.done(delivered)).task.status, 'stopped');
  await app.request(`/tasks/${active.id}/stop`, {});
  await app.stop();
  await app.start();
  assert.equal((await app.request('/schedules')).schedules[0].state, 'paused');
  assert.equal(app.provider.requests.length, 1);
  assert.equal((await app.state()).tasks.length, 2);
});
