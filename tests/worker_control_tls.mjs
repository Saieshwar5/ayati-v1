import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { request } from 'node:https';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { root, until } from './fixtures/polling.mjs';

const options = { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 120000 };
const ready = app => until(() => Number(app.sql("SELECT count(*) FROM relay_executor_slots WHERE state='ready'")) > 0,
  'Executor did not report ready slots over TLS', 35000);

test('executors reach worker control over TLS signed by the private authority, and nothing else connects', options, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, tls: true });
  const executor = await app.startExecutor(); await ready(app);
  const job = await app.send('A private-network request');
  await app.done(job);
  app.pass('A pooled executor registers, receives an offer and completes a reply over TLS');

  // Valid TLS alone grants nothing: machine routes still need credentials.
  const ca = await readFile(app.control.ca);
  const status = await new Promise((resolve, reject) => request({ host: 'localhost', port: app.control.port,
    path: '/v1/executors/register', method: 'POST', ca, headers: { 'Content-Type': 'application/json' } },
  response => { response.resume(); resolve(response.statusCode); }).on('error', reject).end('{}'));
  assert.equal(status, 401);

  const other = join(app.data, 'other-authority');
  execFileSync(join(root, 'tools/worker-control-certs.sh'), [other, 'localhost'], { stdio: 'ignore' });
  for (const [name, extra] of [
    ['another authority', { AYATI_WORKER_CONTROL_CA: join(other, 'ca.pem') }],
    ['plain HTTP to the TLS port', { AYATI_WORKER_CONTROL_URL: `http://127.0.0.1:${app.control.port}`, AYATI_WORKER_CONTROL_CA: undefined }],
  ]) {
    const refused = await app.startExecutor(extra);
    await until(() => refused.child.exitCode !== null, `Executor using ${name} kept running`, 30000);
    assert.notEqual(refused.child.exitCode, 0, name);
  }
  assert.equal(app.sql('SELECT count(*) FROM relay_executors'), '1');
  await app.stop(executor);
  app.pass('Machine routes deny uncredentialed TLS requests; executors trusting another authority or using plain HTTP never register');
});
