// Opt-in: two API replicas behind a proxy with executors on the shared
// `worker-control` name, as on AWS. No sign-in is needed: hosted sessions are
// rechecked with WorkOS, so this test covers transport, failover and draining.
// Lease continuity across a controller change is covered by the process tests
// (multi_controller, executor_pool) using the same reconnecting client.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { request as httpRequest } from 'node:http';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { composeStack } from './fixtures/compose_stack.mjs';
import { until } from './fixtures/polling.mjs';

test('executors fail over between API replicas and rolling replacement drops no request', {
  timeout: 420000, skip: process.env.AYATI_CONTAINER_TEST !== '1',
}, async t => {
  const stack = await composeStack(t, {
    prefix: 'container-replicas', files: ['deploy/compose.replicas.yaml'],
    env: name => ({ AYATI_UPSTREAMS: `${name}-api-1:8765 ${name}-api-2:8765` }),
  });
  const { name, port, compose, docker, sql, pass } = stack;
  await stack.migrate();
  compose(['up', '-d', '--scale', 'api=2', '--scale', 'executor=1', 'api', 'executor', 'proxy']);
  await stack.healthy('api');
  await until(() => Number(sql("SELECT count(*) FROM relay_executor_slots WHERE state='ready'")) >= 2,
    'Executor did not report ready slots', 120000);
  await stack.healthy('executor');
  const replica = index => `${name}-api-${index}`;
  const page = () => new Promise((resolve, reject) => httpRequest({ host: '127.0.0.1', port, path: '/index.html',
    headers: { Host: 'workspace.example.test', Connection: 'close' } }, response => {
    response.resume(); response.on('end', () => resolve(response.statusCode));
  }).on('error', reject).end());
  for (let i = 0; i < 6; i += 1) assert.equal(await page(), 200);
  pass('The proxy serves the website from both API replicas');

  // The executor's health file records when a controller last accepted its
  // capacity report. After its replica dies it must move past the kill time.
  const executor = compose(['ps', '-q', 'executor']);
  const reportedAt = () => JSON.parse(compose(['exec', '-T', 'executor', 'cat', '/var/lib/ayati/executor-health.json'])).reported_at;
  const failovers = [];
  for (const index of [1, 2]) {
    const killed = Math.floor(Date.now() / 1000) + 1;
    docker(['kill', replica(index)]);
    await until(() => reportedAt() > killed + 2, `Executor did not report after replica ${index} was killed`, 60000);
    failovers.push(reportedAt() - killed);
    compose(['exec', '-T', 'executor', '/opt/ayati/ayati', 'executor-health']);
    docker(['start', replica(index)]);
    await stack.healthy('api');
  }
  assert.equal(compose(['ps', '-q', 'executor']), executor);
  assert.equal(docker(['inspect', '--format', '{{.RestartCount}}', executor]), '0');
  pass('Killing either API replica strands no executor: it reconnects through the shared name without restarting');

  // Rolling replacement: steady requests through the proxy while each replica drains.
  let stop = false, served = 0; const failures = [];
  const load = (async () => {
    while (!stop) {
      try { const status = await page(); if (status === 200) served += 1; else failures.push(status); }
      catch (error) { failures.push(error.code ?? 'error'); }
      await new Promise(resolve => setTimeout(resolve, 100));
    }
  })();
  for (const index of [1, 2]) {
    docker(['stop', '-t', '30', replica(index)]);
    docker(['start', replica(index)]);
    await stack.healthy('api');
  }
  stop = true; await load;
  assert.deepEqual(failures, [], `Requests failed during the rolling replacement: ${failures.join(',')}`);
  assert.ok(served > 20);
  await until(() => reportedAt() > Math.floor(Date.now() / 1000) - 10, 'Executor stopped reporting after the replacement', 30000);
  pass(`A rolling replacement of both API replicas dropped none of ${served} requests`);
  await writeFile(join(stack.data, 'replicas-evidence.json'), JSON.stringify({ failover_seconds: failovers, rolling_requests: served }, null, 2));
  console.log('Executor reconnect after replica loss (s):', failovers.join(', '));
});
