// MG4: platform administrators see provider traffic, failures, speed, costs and margin,
// and catalog routes. A slow answer for an old period never replaces the current one.
import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const line = (key, calls, charge, cost) => ({ key, calls, after_fallback: 1, tokens: calls * 150, charge_usd: charge, cost_usd: cost, margin_usd: charge - cost, latency_avg_ms: 420, latency_p95_ms: 1300 });
const report = hours => ({ hours, providers: [line('fireworks', hours === 24 ? 3 : 30, 0.003, 0.0018), line('openrouter', 1, 0.001, 0.002)], models: [line('llama-70b', 4, 0.004, 0.0038)],
  failures: [{ provider: 'together', status: 503, count: 2, last_ms: 0 }, { provider: 'together', status: 0, count: 1, last_ms: 0 }],
  resting: [{ provider: 'together', seconds: 12 }],
  catalog: { on: true, models: [{ id: 'llama-70b', name: 'Llama 70B', status: 'available', price: { input: 0.9, output: 0.9 },
    routes: [{ provider: 'fireworks', model: 'accounts/fireworks/models/llama', priority: 1, weight: 80, cost: { input: 0.6, output: 0.6 }, margin: { input: 0.3, output: 0.3 } },
      { provider: 'openrouter', model: 'meta-llama/llama', priority: 2, weight: 100, cost: { input: 1, output: 1 }, margin: { input: -0.1, output: -0.1 } }] }] } });

test('Operators see providers, failures, routes and losses; a stale period never wins', async () => {
  const pending = [];
  await reactFixture(async ({ render, change, update }) => {
    const { ModelOperations } = await import('../src/react/access/model-operations.tsx');
    await render(ModelOperations);
    const day = pending.find(p => p.path.endsWith('hours=24'));
    await change(document.querySelector('select'), '168');
    await update(() => pending.find(p => p.path.endsWith('hours=168')).resolve(response(report(168))));
    await update(() => day.resolve(response(report(24))));
    const text = document.body.textContent;
    assert.match(document.querySelector('caption').parentElement.textContent, /fireworks30/, 'the 7-day answer stays');
    assert.match(text, /Resting after a failure on this controller: together \(12s\)/);
    assert.match(text, /together503 *2/);
    assert.match(text, /togetherUnreachable1/);
    assert.match(text, /Llama 70Bfireworksaccounts\/fireworks\/models\/llama180/);
    assert.equal(document.querySelectorAll('.model-operations-loss').length, 2, 'the openrouter row and route lose money');
  }, { url: 'http://localhost/?view=account&admin=1', fetch: path => new Promise(resolve => pending.push({ path, resolve })) });
});

test('A broken catalog is reported, not hidden', async () => {
  await reactFixture(async ({ render }) => {
    const { ModelOperations } = await import('../src/react/access/model-operations.tsx');
    await render(ModelOperations);
    assert.match(document.body.textContent, /The model catalog can't be read: Model "x" has 1 to 8 routes/);
    assert.match(document.body.textContent, /No calls in this period/);
  }, { url: 'http://localhost/?view=account&admin=1', fetch: async () => response({ hours: 24, providers: [], models: [], failures: [], resting: [], catalog: { on: true, error: 'Model "x" has 1 to 8 routes', models: [] } }) });
});
