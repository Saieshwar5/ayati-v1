import { test } from 'node:test';
import assert from 'node:assert/strict';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/polling.mjs';

const options = { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 120000 };

test('a stopping controller reports draining while serving, exits cleanly and leaves the active reply to finish', options, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, resumableHold: true,
    apiEnv: { AYATI_DRAIN_SECONDS: '2' } });
  const executor = await app.startExecutor();
  await until(() => Number(app.sql("SELECT count(*) FROM relay_executor_slots WHERE state='ready'")) > 0,
    'Executor did not report ready slots', 30000);
  app.provider.setHeld(true);
  const job = await app.send('DRAIN_SENTINEL: bounded request during a controller stop');
  await until(() => app.provider.requests.length > 0, 'The reply did not start on the executor');
  const health = path => fetch(app.base + path, { headers: { Connection: 'close' } })
    .then(async response => [response.status, await response.text()]);
  assert.deepEqual(await health('/health/ready'), [200, 'ready']);
  const began = Date.now();
  const stopping = app.stopApi(); // Asserts exit code 0.
  await until(async () => (await health('/health/ready'))[0] === 503, 'Readiness did not report draining');
  assert.deepEqual(await health('/health/ready'), [503, 'draining']);
  assert.equal((await health('/health/live'))[0], 200);
  // Requests are still served during the drain delay.
  assert.ok((await app.history()).some(message => message.id === job.reply));
  await stopping;
  assert.ok(Date.now() - began >= 2000, 'The controller closed before its drain delay');
  app.pass('A stopping controller reports draining while still serving, then exits with code 0');
  await app.startApi();
  app.provider.setHeld(false);
  await app.done(job);
  await app.stop(executor);
  app.pass('The reply that was running during the controller stop finishes after a replacement starts');
});
