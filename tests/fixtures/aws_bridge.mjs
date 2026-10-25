import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { join, resolve } from 'node:path';
import { randomBytes } from 'node:crypto';
import { until } from './goose_app.mjs';

export const cloudEnvironment = {
  AYATI_CLOUD_WORKERS: '1',
  AYATI_AWS_EXECUTOR_ROLE_ARN: 'arn:aws:iam::123456789012:role/ayati-test-executor',
  AYATI_AWS_EXECUTOR_CLUSTER_ARN: 'arn:aws:ecs:ap-south-1:123456789012:cluster/ayati-test',
  AYATI_AWS_EXECUTOR_RELEASE_ARN: 'arn:aws:ecs:ap-south-1:123456789012:task-definition/ayati-test:1',
};

export async function bridgeFixture(t, app) {
  const directory = join(app.data, 'signed-bridge');
  await mkdir(directory, { mode: 0o700 });
  const config = join(directory, 'config.json');
  const modelInfo = join(directory, 'model-info.json');
  await writeFile(modelInfo, JSON.stringify({ url: app.provider.url,
    key: app.provider.key ?? randomBytes(32).toString('base64url') }), { mode: 0o600 });
  await writeFile(config, JSON.stringify({ directory, control_url: app.control.url,
    bootstrap_file: app.control.keyFile, model_info: modelInfo }), { mode: 0o600 });
  const child = spawn(process.env.AYATI_TEST_BRIDGE_PYTHON ?? 'python3',
    [resolve('tests/fixtures/aws_gateway.py'), config], {
      cwd: app.data, env: { PATH: process.env.PATH, LANG: 'C.UTF-8', AWS_EC2_METADATA_DISABLED: 'true' },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
  const record = { child, logs: '' };
  child.stdout.on('data', bytes => { record.logs += bytes; });
  child.stderr.on('data', bytes => { record.logs += bytes; });
  t.after(async () => {
    await app.stop(record);
    await writeFile(join(directory, 'bridge.log'), record.logs);
  });
  const settings = await until(async () => {
    assert.equal(child.exitCode, null, record.logs);
    try { return JSON.parse(await readFile(join(directory, 'bridge-ready.json'), 'utf8')); }
    catch (error) { if (error.code === 'ENOENT') return false; throw error; }
  }, 'Local signed bridge did not start', 15000);
  return settings;
}

export async function captureStarts(t, url) {
  // Keep synthetic capabilities in memory; never persist them in test reports.
  const starts = [];
  const server = createServer(async (request, response) => {
    const stop = new AbortController();
    response.on('close', () => stop.abort());
    try {
      const chunks = [];
      for await (const chunk of request) chunks.push(chunk);
      const body = Buffer.concat(chunks);
      const authorization = request.headers.authorization;
      const upstream = await fetch(url + request.url, { method: request.method,
        headers: { Authorization: authorization, 'Content-Type': 'application/json', Connection: 'close' },
        body: request.method === 'GET' ? undefined : body,
        signal: stop.signal,
      });
      if (request.url.endsWith('/start') && upstream.ok) {
        starts.push({ path: request.url, body: JSON.parse(body), token: authorization.slice(7) });
      }
      response.writeHead(upstream.status, { 'Content-Type': upstream.headers.get('content-type') ?? 'application/json' });
      const reader = upstream.body.getReader();
      response.on('close', () => { reader.cancel().catch(() => {}); });
      try {
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          response.write(value);
        }
        response.end();
      } finally { reader.releaseLock(); }
    } catch { response.destroy(); }
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  t.after(() => { server.closeAllConnections(); server.close(); });
  return { url: `http://127.0.0.1:${server.address().port}`, starts };
}
