import { test } from 'node:test';
import { createRequire } from 'node:module';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { createServer } from 'node:net';
import { fixtureProvider } from './fixtures/goose_provider.mjs';

const root = resolve(import.meta.dirname, '..');
const binary = process.env.AYATI_TEST_BIN ?? join(root, 'target/debug/ayati');

async function until(check, message, timeout = 20000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    const result = await check();
    if (result) return result;
    await new Promise((resolve) => setTimeout(resolve, 30));
  }
  throw new Error(message);
}

async function setup(t, proofTool = false) {
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
  const env = { ...process.env, AYATI_RUNTIME: 'goose', AYATI_DATA_DIR: data,
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
        timer = setTimeout(() => { child.kill('SIGKILL'); reject(new Error('Shutdown timed out')); }, 8000);
      })]);
    } finally { clearTimeout(timer); }
  }
  t.after(async () => { await stop(); provider.close(); if (t.passed) await rm(data, { recursive: true, force: true }); else console.error('Failed fixture data:', data, logs); });
  await start();
  return { data, base, request, provider, start, stop, configure: (changes) => Object.assign(env, changes),
    state: () => request('/state'),
    submit: (content) => request('/messages', { content }),
    done: (id) => until(async () => {
      const state = await request('/state');
      const task = state.tasks.find((r) => r.id === id);
      if (['completed', 'failed', 'stopped'].includes(task?.status)) return { task, state };
    }, 'Run did not finish'),
  };
}

test('real pinned Goose streams, persists scoped identity and loads history without replay', { timeout: 60000 }, async (t) => {
  const app = await setup(t);
  const health = await app.request('/health');
  assert.equal(health.runtime, 'goose');
  const input = await app.submit('hello');
  const { task, state } = await app.done(input.id);
  assert.equal(task.status, 'completed', task.detail);
  assert.equal(state.messages.find((m) => m.role === 'assistant').content, 'GOOSE_FIXTURE_OK 🦀');
  assert.ok(app.provider.requests.length > 0);
  assert.deepEqual(app.provider.requests.at(-1).tools ?? [], []);
  const db = new DatabaseSync(join(app.data, 'ayati.sqlite'));
  const session = db.prepare('SELECT * FROM runtime_sessions').get();
  assert.ok(session.session_id);
  assert.equal(db.prepare('SELECT conversation_id FROM run_scopes WHERE task_id=?').get(input.id).conversation_id, 'local-conversation');
  db.close();
  const count = app.provider.requests.length;
  await app.stop();
  await app.start();
  const followup = await app.submit('followup');
  const result = await app.done(followup.id);
  assert.equal(result.task.status, 'completed', result.task.detail);
  assert.equal(result.state.messages.filter((m) => m.role === 'assistant').length, 2);
  assert.equal(app.provider.requests.length, count + 1);
  assert.match(JSON.stringify(app.provider.requests.at(-1).messages), /hello/);
});

test('real Goose MCP actions require an exact Allow once or Deny; decisions cannot be reused', { timeout: 60000 }, async (t) => {
  const app = await setup(t, true);
  for (const allow of [true, false]) {
    const input = await app.submit(`tool ${allow ? 'allow' : 'deny'}`);
    const permission = await until(async () => {
      const s = await app.state();
      const failed = s.tasks.find((r) => r.id === input.id && r.status === 'failed');
      if (failed) throw new Error(failed.detail);
      if (s.tasks.some((r) => r.id === input.id && r.status === 'completed')) {
        throw new Error('No permission before completion: ' + JSON.stringify({ messages: s.messages, tools: app.provider.requests.map((r) => r.tools?.map((t) => t.function.name)) }));
      }
      return s.permissions?.find((p) => p.task_id === input.id);
    }, 'No permission requested');
    const schemas = app.provider.requests.at(-1).tools.map((tool) => tool.function.name);
    assert.deepEqual(schemas, ['ayati_proof__get_demo_business']);
    await app.request(`/permissions/${permission.id}`, { allow });
    const { task, state } = await app.done(input.id);
    assert.equal(task.status, 'completed', task.detail);
    const output = state.messages.filter((m) => m.task_id === input.id && m.role === 'assistant').map((m) => m.content).join('');
    if (allow) assert.match(output, /Stock: 12/);
    else assert.doesNotMatch(output, /Stock: 12/);
    const reused = await fetch(app.base + `/permissions/${permission.id}`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ allow: true }),
    });
    assert.equal(reused.status, 400);
  }
});

test('Stop cancels a stalled real Goose request and later input remains usable', { timeout: 60000 }, async (t) => {
  const app = await setup(t);
  const input = await app.submit('stall');
  await until(() => app.provider.requests.length > 0, 'Provider did not receive prompt');
  await app.request(`/tasks/${input.id}/stop`, {});
  assert.equal((await app.done(input.id)).task.status, 'stopped');
  const next = await app.submit('continue');
  const { task } = await app.done(next.id);
  assert.equal(task.status, 'completed', task.detail);
});

test('Stop invalidates pending owner approval and it remains invalid after restart', { timeout: 60000 }, async (t) => {
  const app = await setup(t, true);
  const input = await app.submit('tool wait');
  const permission = await until(async () => (await app.state()).permissions?.[0], 'No approval');
  await app.request(`/tasks/${input.id}/stop`, {});
  await app.done(input.id);
  await app.stop();
  await app.start();
  const response = await fetch(app.base + `/permissions/${permission.id}`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ allow: true }),
  });
  assert.equal(response.status, 400);
  assert.deepEqual((await app.state()).permissions, []);
});


test('real browser operates Goose chat, approval and Stop through the web app', { timeout: 60000 }, async (t) => {
  const app = await setup(t, true);
  const { chromium } = createRequire(new URL('../web/package.json', import.meta.url))('@playwright/test');
  const browser = await chromium.launch();
  t.after(() => browser.close());
  const page = await browser.newPage();
  await page.goto(app.base.replace('/api', ''));
  await page.getByRole('textbox', { name: 'Message Ayati' }).fill('tool allow');
  await page.getByRole('button', { name: 'Send message' }).click();
  await page.getByRole('button', { name: 'Allow once', exact: true }).click();
  await page.getByText(/Stock: 12/).waitFor();
  await until(async () => (await app.state()).tasks.every((r) => r.status === 'completed'), 'UI tool run did not complete');
  await page.getByRole('textbox', { name: 'Message Ayati' }).fill('stall');
  await page.getByRole('button', { name: 'Send message' }).click();
  await until(() => app.provider.requests.some((r) => JSON.stringify(r.messages).includes('stall')), 'Stalled request did not start');
  await page.getByRole('button', { name: 'Stop', exact: true }).click();
  await page.getByText('Stopped. Any files already created are still available.').waitFor();
  assert.equal((await app.state()).tasks[0].status, 'stopped');
});


test('changed tool grants cannot silently reload a saved session; startup failure closes the reply', { timeout: 60000 }, async (t) => {
  const app = await setup(t, true);
  const first = await app.submit('hello');
  assert.equal((await app.done(first.id)).task.status, 'completed');
  const calls = app.provider.requests.length;
  await app.stop();
  app.configure({ AYATI_PROOF_TOOL: '0' });
  await app.start();
  const changed = await app.submit('hello again');
  const { task, state } = await app.done(changed.id);
  assert.equal(task.status, 'failed');
  assert.match(task.detail, /settings differ/);
  assert.equal(app.provider.requests.length, calls);
  assert.equal(state.messages.find((m) => m.task_id === changed.id && m.role === 'assistant').state, 'failed');
});
