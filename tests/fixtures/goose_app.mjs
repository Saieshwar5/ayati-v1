import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createServer } from 'node:net';
import { fixtureProvider } from './goose_provider.mjs';

export const root = resolve(import.meta.dirname, '../..');
export const vmManifest = process.env.AYATI_VM_TEST_MANIFEST;
const binary = process.env.AYATI_TEST_BIN ?? join(root, 'target/debug/ayati');

export async function until(check, message, timeout = vmManifest ? 45000 : 20000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    const result = await check();
    if (result) return result;
    await new Promise((resolve) => setTimeout(resolve, 30));
  }
  throw new Error(message);
}

export async function setup(t, proofTool = false) {
  const data = await mkdtemp(join(tmpdir(), 'ayati-goose-test-'));
  const provider = await fixtureProvider();
  const portServer = createServer();
  portServer.listen(0, '127.0.0.1');
  await once(portServer, 'listening');
  const port = portServer.address().port;
  await new Promise((resolve) => portServer.close(resolve));
  const base = `http://127.0.0.1:${port}/api`;
  let child;
  let logs = '';
  const env = { ...process.env, AYATI_RUNTIME: 'goose', AYATI_EXECUTION: vmManifest ? 'vm' : 'host', AYATI_VM_MANIFEST: vmManifest ?? '', AYATI_DATA_DIR: data,
    AYATI_PORT: String(port), AYATI_MODEL: 'fixture', AYATI_GOOSE_PROVIDER: 'openai',
    OPENAI_API_KEY: 'fixture-not-a-secret', OPENAI_HOST: provider.url,
    AYATI_PROOF_TOOL: proofTool ? '1' : '0',
    AYATI_GOOSE_BIN: join(root, '.cache/goose/v1.54.0/goose'),
    RUST_LOG: 'ayati=info' };
  async function request(path, body) {
    const response = await fetch(base + path, body === undefined ? undefined : {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body),
    });
    assert.equal(response.ok, true, await response.clone().text());
    return response.json();
  }
  async function start() {
    child = spawn(binary, ['serve'], { cwd: root, env, stdio: ['ignore', 'pipe', 'pipe'] });
    child.stdout.on('data', (v) => { logs += v; });
    child.stderr.on('data', (v) => { logs += v; });
    await until(async () => {
      if (child.exitCode !== null) throw new Error(`Daemon exited: ${logs}`);
      try { return (await fetch(base + '/health')).ok; } catch { return false; }
    }, 'Daemon did not start');
  }
  async function stop() {
    if (!child || child.exitCode !== null) return;
    const exited = once(child, 'exit');
    await request('/shutdown', {});
    let timer;
    try {
      await Promise.race([exited, new Promise((_, reject) => {
        timer = setTimeout(() => { child.kill('SIGKILL'); reject(new Error('Shutdown timed out')); }, vmManifest ? 25000 : 8000);
      })]);
    } finally { clearTimeout(timer); }
  }
  t.after(async () => { await stop(); provider.close(); if (t.passed) await rm(data, { recursive: true, force: true }); else console.error('Failed fixture data:', data, logs); });
  await start();
  return { data, base, request, provider, start, stop,
    crash: async () => { const exited = once(child, 'exit'); child.kill('SIGKILL'); await exited; },
    childPid: () => child.pid, configure: (changes) => Object.assign(env, changes),
    state: () => request('/state'),
    submit: (content) => request('/messages', { content }),
    done: (id) => until(async () => {
      const state = await request('/state');
      const task = state.tasks.find((r) => r.id === id);
      if (['completed', 'failed', 'stopped', 'interrupted'].includes(task?.status)) return { task, state };
    }, 'Run did not finish'),
  };
}
