import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const ok = data => ({ ok: true, status: 200, json: async () => data });
const fail = (error, status = 400) => ({ ok: false, status, json: async () => ({ error }) });
const session = { email: 'op@example.com', name: 'Sai', environment: 'production', can_store_keys: true };
const health = { calls: 120, errors: 2, p95_ms: 4100 };
const providers = {
  can_store_keys: true,
  providers: [
    { name: 'fireworks', base_url: 'https://api.fireworks.ai/inference/v1', source: 'console', enabled: true, key_hint: '3f9a', key_rotated_ms: Date.now(), switch: null, health, models: [] },
    { name: 'openai', base_url: 'https://api.openai.com/v1', source: 'deployment', enabled: true, switch: null, health, models: [] },
  ],
};
const route = { model: 'kimi-k2', main_provider: 'fireworks', main_model: 'kimi-k2-instruct', second_provider: 'openai', second_model: 'gpt-5-mini',
  main_percent: 100, timeout_seconds: 30, changed_ms: Date.now(), by: 'Sai', traffic: [] };
const routes = { routes: [route], requested: [{ model: 'kimi-k2', agents: 3, calls_7d: 900 }] };

function server(overrides = {}) {
  const writes = [];
  const fetch = async (path, options = {}) => {
    if (options.method === 'POST') {
      assert.equal(options.headers['X-Ayati-Request'], 'identity');
      const body = JSON.parse(options.body);
      writes.push({ path, body });
      if (overrides.post) return overrides.post(path, body, writes);
      return ok(path.includes('/routes') ? routes : providers);
    }
    if (overrides.get) { const answer = overrides.get(path); if (answer) return answer; }
    if (path.endsWith('/admin/session')) return ok(session);
    if (path.endsWith('/admin/providers')) return ok(providers);
    if (path.endsWith('/admin/routes')) return ok(routes);
    if (path.endsWith('/admin/connector-providers')) return ok({ providers: [], configured: [] });
    if (path.endsWith('/admin/workspace-codes')) return ok({ codes: [] });
    return ok({});
  };
  return { writes, fetch };
}

const textarea = () => document.querySelector('dialog[open] textarea');
const confirmButton = label => [...document.querySelectorAll('dialog[open] button[type=submit]')].find(b => b.textContent === label);

test('people who are not operators see why, and nothing of the console', async () => {
  await reactFixture(async ({ render }) => {
    const { ControlApp } = await import('../src/control/app.tsx'); await render(ControlApp);
    assert.match(document.body.textContent, /Cuckoo Control is for Cuckoo operators/);
    assert.match(document.body.textContent, /AYATI_PLATFORM_ADMINS/);
    assert.equal(document.querySelector('nav'), null);
  }, { url: 'http://localhost/control.html', fetch: async () => fail('Only Cuckoo operators can open Cuckoo Control', 403) });
});

test('the environment band and live provider strip show on every screen', async () => {
  const { fetch } = server();
  await reactFixture(async ({ render }) => {
    const { ControlApp } = await import('../src/control/app.tsx'); await render(ControlApp);
    assert.ok(document.querySelector('.cc-root.env-production'));
    assert.match(document.querySelector('.cc-env').textContent, /PRODUCTION/);
    const strip = document.querySelector('[aria-label="Live provider status"]').textContent;
    assert.match(strip, /fireworks/); assert.match(strip, /120 calls\/h · 1\.7% err · p95 4\.1 s/);
  }, { url: 'http://localhost/control.html?section=codes', fetch });
});

test('turning a provider off needs a reason, shows the impact and sends once', async () => {
  let release;
  const { writes, fetch } = server({ post: () => new Promise(resolve => { release = () => resolve(ok({ ...providers,
    providers: [{ ...providers.providers[0], enabled: false, switch: { note: 'Fireworks incident', changed_ms: Date.now(), by: 'Sai' } }, providers.providers[1]] })); }) });
  await reactFixture(async ({ render, button, click, change, submit, update }) => {
    const { ControlApp } = await import('../src/control/app.tsx'); await render(ControlApp);
    await click(button('Turn off fireworks…'));
    assert.match(document.querySelector('dialog[open]').textContent, /kimi-k2: all traffic to openai/);
    assert.equal(confirmButton('Turn off fireworks').disabled, true, 'no reason yet');
    await change(textarea(), 'no');
    assert.equal(confirmButton('Turn off fireworks').disabled, true, 'too short');
    await change(textarea(), 'Fireworks incident');
    const form = document.querySelector('dialog[open] form');
    await submit(form); await submit(form);
    assert.equal(writes.length, 1, 'a double submit sends once');
    assert.deepEqual(writes[0], { path: '/api/identity/admin/providers/fireworks/switch', body: { enabled: false, reason: 'Fireworks incident' } });
    await update(() => release());
    assert.equal(document.querySelector('dialog[open]'), null);
    assert.match(document.body.textContent, /Off since .*Fireworks incident/);
  }, { url: 'http://localhost/control.html?section=providers&id=fireworks', fetch });
});

test('a failed save keeps the dialog and the reason', async () => {
  const { writes, fetch } = server({ post: () => fail('Routes still use fireworks; change them first') });
  await reactFixture(async ({ render, button, click, change, submit }) => {
    const { ControlApp } = await import('../src/control/app.tsx'); await render(ControlApp);
    await click(button('Remove provider'));
    await change(textarea(), 'Moving to another account');
    await submit(document.querySelector('dialog[open] form'));
    assert.equal(writes.length, 1);
    assert.match(document.querySelector('dialog[open] [role=alert]').textContent, /Routes still use fireworks/);
    assert.equal(textarea().value, 'Moving to another account');
  }, { url: 'http://localhost/control.html?section=providers&id=fireworks', fetch });
});

test('a route change waits in the tray until reviewed and applied', async () => {
  const { writes, fetch } = server();
  await reactFixture(async ({ render, button, click, change, submit }) => {
    const { ControlApp } = await import('../src/control/app.tsx'); await render(ControlApp);
    assert.equal(document.querySelector('[aria-label="Changes not applied"]'), null);
    await click(button('90 / 10'));
    const tray = document.querySelector('[aria-label="Changes not applied"]');
    assert.match(tray.textContent, /100% fireworks:kimi-k2-instruct .* → 90% fireworks:kimi-k2-instruct · 10% openai:gpt-5-mini/);
    assert.equal(writes.length, 0, 'nothing is live yet');
    await click(button('Discard'));
    assert.equal(document.querySelector('[aria-label="Changes not applied"]'), null);
    await click(button('70 / 30'));
    await click(button('Review and apply'));
    assert.match(document.querySelector('dialog[open] .cc-diff').textContent, /− 100%.*\+ 70%/s);
    await change(textarea(), 'Fireworks is slow; spread the load');
    await submit(document.querySelector('dialog[open] form'));
    assert.deepEqual(writes[0], { path: '/api/identity/admin/routes', body: {
      model: 'kimi-k2', main_provider: 'fireworks', main_model: 'kimi-k2-instruct', second_provider: 'openai', second_model: 'gpt-5-mini',
      main_percent: 70, timeout_seconds: 30, reason: 'Fireworks is slow; spread the load' } });
  }, { url: 'http://localhost/control.html?section=routing&id=kimi-k2', fetch });
});

test('suspending a workspace states what happens and refreshes the detail', async () => {
  const detail = { id: 'w1', name: 'Northwind Legal', created_ms: Date.now(), owner: 'a.khan@northwind.test', people: 3, agents: 2, running: 1,
    spend: 41.75, cap: 45, own_cap: true, calls_7d: [1, 2, 3, 4, 5, 6, 7], error_rate_7d: 0, last_active_ms: Date.now(), status: 'active',
    admins: 1, connections: 2, library_items: 96, library_bytes: 310e6, problem_reports_30d: 0, code: 'Northwind', suspended_ms: null,
    suspended_note: null, agents_paused_ms: null, models: ['gpt-5-mini'], log: [] };
  const { writes, fetch } = server({
    get: path => path.includes('/admin/workspaces/w1') ? ok(detail) : path.includes('/admin/workspaces') ? ok({ workspaces: [detail], total: 1 }) : null,
    post: () => ok({ ...detail, status: 'suspended', suspended_ms: Date.now(), suspended_note: 'Chargeback' }),
  });
  await reactFixture(async ({ render, button, click, change, submit }) => {
    const { ControlApp } = await import('../src/control/app.tsx'); await render(ControlApp);
    assert.match(document.body.textContent, /\$41\.75 \/ \$45/);
    await click(button('Suspend workspace…'));
    assert.match(document.querySelector('dialog[open]').textContent, /None of its 3 members can open it.*1 running request/s);
    await change(textarea(), 'Chargeback');
    await submit(document.querySelector('dialog[open] form'));
    assert.deepEqual(writes[0], { path: '/api/identity/admin/workspaces/w1/suspend', body: { reason: 'Chargeback' } });
    assert.ok(button('Lift the suspension'));
  }, { url: 'http://localhost/control.html?section=workspaces&id=w1', fetch });
});
