import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const ok = data => ({ ok: true, status: 200, json: async () => data });
const fail = (error, status = 400) => ({ ok: false, status, json: async () => ({ error }) });
const session = { email: 'op@example.com', name: 'Sai', environment: 'production', can_store_keys: true, catalog_on: true };
const health = { calls: 120, errors: 2, p95_ms: 4100 };
const providers = {
  can_store_keys: true,
  providers: [
    { name: 'fireworks', base_url: 'https://api.fireworks.ai/inference/v1', source: 'deployment', enabled: true, key_in_console: true, key_hint: '3f9a',
      key_rotated_ms: Date.now(), switch: null, health, routes: [{ model: 'kimi-k2', provider_model: 'accounts/fireworks/models/kimi-k2', priority: 1 },
        { model: 'qwen3', provider_model: 'accounts/fireworks/models/qwen3', priority: 1 }] },
    { name: 'openai', base_url: 'https://api.openai.com/v1', source: 'deployment', enabled: true, key_in_console: false, switch: null, health,
      routes: [{ model: 'kimi-k2', provider_model: 'gpt-5-mini', priority: 2 }] },
  ],
};
const operations = { hours: 24, providers: [], failures: [{ provider: 'fireworks', status: 503, count: 4, last_ms: Date.now() }],
  models: [{ key: 'kimi-k2', calls: 900, after_fallback: 12, tokens: 1, charge_usd: 9, cost_usd: 6, margin_usd: 3, latency_avg_ms: 2000, latency_p95_ms: 4100 }],
  resting: [{ provider: 'openai', seconds: 21 }],
  catalog: { on: true, models: [{ id: 'kimi-k2', name: 'Kimi K2', status: 'available', price: { input: 0.9, output: 2.5 }, routes: [
    { provider: 'fireworks', model: 'accounts/fireworks/models/kimi-k2', priority: 1, weight: 80, cost: { input: 0.6, output: 2.5 }, margin: { input: 0.3, output: 0 } },
    { provider: 'openai', model: 'gpt-5-mini', priority: 2, weight: 100, cost: { input: 1.2, output: 2 }, margin: { input: -0.3, output: 0.5 } }] }] } };

function server(overrides = {}) {
  const writes = [];
  const fetch = async (path, options = {}) => {
    if (options.method === 'POST') {
      assert.equal(options.headers['X-Ayati-Request'], 'identity');
      const body = JSON.parse(options.body);
      writes.push({ path, body });
      if (overrides.post) return overrides.post(path, body, writes);
      return ok(providers);
    }
    if (overrides.get) { const answer = overrides.get(path); if (answer) return answer; }
    if (path.endsWith('/admin/session')) return ok(session);
    if (path.endsWith('/admin/providers')) return ok(providers);
    if (path.includes('/admin/models')) return ok(operations);
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
    assert.match(strip, /fireworks/); assert.match(strip, /120 calls\/h · 1\.6% err · p95 4\.1 s/);
  }, { url: 'http://localhost/control.html?section=codes', fetch });
});

test('turning a provider off needs a reason, shows the impact and sends once', async () => {
  let release;
  const { writes, fetch } = server({ post: () => new Promise(resolve => { release = () => resolve(ok({ ...providers,
    providers: [{ ...providers.providers[0], enabled: false, switch: { note: 'Fireworks incident', changed_ms: Date.now(), by: 'Sai' } }, providers.providers[1]] })); }) });
  await reactFixture(async ({ render, button, click, change, submit, update }) => {
    const { ControlApp } = await import('../src/control/app.tsx'); await render(ControlApp);
    await click(button('Turn off fireworks…'));
    const impact = document.querySelector('dialog[open]').textContent;
    assert.match(impact, /kimi-k2: served by openai/);
    assert.match(impact, /qwen3: fails — no other provider is on/);
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
    await click(button('Use its key file again'));
    await change(textarea(), 'Moving to another account');
    await submit(document.querySelector('dialog[open] form'));
    assert.equal(writes.length, 1);
    assert.match(document.querySelector('dialog[open] [role=alert]').textContent, /Routes still use fireworks/);
    assert.equal(textarea().value, 'Moving to another account');
  }, { url: 'http://localhost/control.html?section=providers&id=fireworks', fetch });
});

test('routing shows the catalog live: order, losses, switched-off and resting providers', async () => {
  const off = { ...providers, providers: [{ ...providers.providers[0], enabled: false }, providers.providers[1]] };
  const { writes, fetch } = server({ get: path => path.endsWith('/admin/providers') ? ok(off) : null });
  await reactFixture(async ({ render }) => {
    const { ControlApp } = await import('../src/control/app.tsx'); await render(ControlApp);
    const text = document.body.textContent;
    assert.match(text, /Kimi K2/);
    assert.match(text, /900 calls · 12 after fallback · p95 4\.1 s/);
    const rows = [...document.querySelectorAll('tbody tr')].map(r => r.textContent);
    assert.match(rows[0], /fireworks.*1 · first.*SWITCHED OFF/);
    assert.match(rows[1], /openai.*−\$0\.30.*RESTING 21s/);
    assert.ok(document.querySelector('td.bad'), 'a losing route is marked');
    assert.match(text, /fireworks503/);
    assert.equal(writes.length, 0, 'nothing here changes the catalog');
  }, { url: 'http://localhost/control.html?section=routing', fetch });
});

test('suspending a workspace states what happens and refreshes the detail', async () => {
  const detail = { id: 'w1', name: 'Northwind Legal', created_ms: Date.now(), owner: 'a.khan@northwind.test', people: 3, agents: 2, running: 1,
    charge: 41.75, cost: 30, budget: 45, calls_7d: [1, 2, 3, 4, 5, 6, 7], error_rate_7d: 0, last_active_ms: Date.now(), status: 'active',
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
