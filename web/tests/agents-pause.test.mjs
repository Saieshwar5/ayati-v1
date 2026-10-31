import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=activity&tab=now`;
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const running = { paused: false, since_ms: null, by: null, note: null, active: 2 };
const paused = { paused: true, since_ms: Date.now(), by: 'Ravi Kumar', note: 'Wrong emails going out', active: 0 };
const session = (role, since = null) => ({ user: { id: 'me', email: 'me@test' }, workspaces: [{ id: w, name: 'Store', role, agents_paused_ms: since }] });

async function banner(render) {
  const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
  const { PauseBanner } = await import('../src/react/activity/agents-pause.tsx');
  await render(PauseBanner);
}

test('every member sees the pause, with who and why; only admins can resume it', async () => {
  for (const role of ['Member', 'Admin']) {
    const writes = [];
    let state = paused;
    await reactFixture(async ({ render, button, click }) => {
      await banner(render);
      const shown = document.querySelector('.agents-paused-banner');
      assert.match(shown.textContent, /All agents are paused by Ravi Kumar since .*“Wrong emails going out”/);
      if (role === 'Member') { assert.equal(button('Resume agents'), undefined); return; }
      await click(button('Resume agents'));
      assert.ok(writes[0].endsWith('/agents/resume'));
      assert.equal(document.querySelector('.agents-paused-banner'), null);
    }, { url, fetch: async (path, options = {}) => {
      if (path.endsWith('/session')) return response(session(role, paused.since_ms));
      if (options.body) { writes.push(path); state = { ...running, active: 0 }; }
      return response(state);
    } });
  }
});

test('a pause by another admin appears at once through the live stream, without polling', async () => {
  const reads = [];
  let stream, stopLive;
  const original = globalThis.EventSource;
  globalThis.EventSource = class { constructor() { this.listeners = new Map(); stream = this; } addEventListener(name, fn) { this.listeners.set(name, fn); } close() {} };
  try {
    await reactFixture(async ({ render, update }) => {
      const { startLive } = await import('../src/react/chat/live.ts');
      await banner(render);
      await update(() => { stopLive = startLive(); });
      assert.equal(document.querySelector('.agents-paused-banner'), null);
      assert.equal(reads.filter(path => path.endsWith('/agents/pause')).length, 0, 'nothing is read while not paused');
      await update(() => stream.listeners.get('changes')({ data: JSON.stringify({ cursor: 7, events: [{ kind: 'agents' }] }) }));
      assert.match(document.querySelector('.agents-paused-banner').textContent, /Wrong emails going out/);
      stopLive?.();
    }, { url, fetch: async path => {
      reads.push(path);
      if (path.endsWith('/session')) return response(session('Member'));
      if (path.endsWith('/message-capabilities')) return response({ live: true });
      return response(paused);
    } });
  } finally { globalThis.EventSource = original; }
});

test('pausing says how many requests stop, sends one request for a double click, and offers Resume', async () => {
  const writes = [];
  let state = running;
  await reactFixture(async ({ render, button, click, change, update }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { PauseAgentsControl } = await import('../src/react/activity/agents-pause.tsx');
    await render(PauseAgentsControl);
    await click(button('Pause all agents'));
    const dialog = document.querySelector('dialog');
    assert.match(dialog.textContent, /2 requests that are running or waiting will be stopped and won’t restart on their own/);
    assert.match(dialog.textContent, /schedules skip their runs/);
    await change(dialog.querySelector('input'), 'Wrong emails going out');
    await update(() => { const confirm = [...dialog.querySelectorAll('button')].find(b => b.textContent === 'Pause all agents'); confirm.click(); confirm.click(); });
    assert.equal(writes.length, 1);
    assert.deepEqual(writes[0].body, { note: 'Wrong emails going out' });
    assert.equal(document.querySelector('dialog'), null);
    assert.ok(button('Resume agents'));
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session('Admin'));
    if (options.body) { writes.push({ path, body: JSON.parse(options.body) }); state = paused; }
    return response(state);
  } });
});

test('members see no pause control', async () => {
  await reactFixture(async ({ render, button }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { PauseAgentsControl } = await import('../src/react/activity/agents-pause.tsx');
    await render(PauseAgentsControl);
    assert.equal(button('Pause all agents'), undefined);
  }, { url, fetch: async path => response(path.endsWith('/session') ? session('Member') : { ...running, active: null }) });
});
