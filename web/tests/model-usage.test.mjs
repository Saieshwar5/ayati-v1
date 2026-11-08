// MG2: model use and charges by agent, model, person and day. A slow answer for an
// old period never replaces the current one; members see only their own use.
import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const row = (key, name, calls, charge) => ({ key, name, calls, input_tokens: calls * 1000, output_tokens: calls * 200, charge_usd: charge });
const report = (days, scope = 'workspace') => ({ days, since_ms: 0, scope, totals: { calls: 12, input_tokens: 12000, output_tokens: 2400, charge_usd: days === 7 ? 0.0042 : 1.5 },
  by_agent: [row('a', 'Meera', 10, 1.2), row('', 'Library', 2, 0.3)], by_model: [row('llama-70b', 'Llama 70B', 12, 1.5)],
  by_person: [row('p', 'Ananya', 12, 1.5)], by_day: [row('2026-11-05', undefined, 12, 1.5)] });
const session = role => ({ user: { id: 'p', email: 'p@test' }, workspaces: [{ id: w, name: 'Store', role }] });

test('Usage shows charges by agent, model, person and day, and a stale period never wins', async () => {
  const pending = [];
  await reactFixture(async ({ render, change, update }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { UsageReportView } = await import('../src/react/usage/usage-report.tsx');
    await render(UsageReportView, { title: 'Workspace usage' });
    const thirty = pending.find(p => p.path.includes('days=30'));
    await change(document.querySelector('select'), '7');
    const seven = pending.find(p => p.path.includes('days=7'));
    await update(() => seven.resolve(response(report(7))));
    await update(() => thirty.resolve(response(report(30))));
    const text = document.body.textContent;
    assert.match(text, /Charges\$0\.0042/, 'the 7-day answer stays after the slow 30-day one');
    assert.equal(document.querySelector('[aria-label=Charges] strong').textContent, '$0.0042');
    for (const caption of ['By agent', 'By model', 'By person', 'By day']) assert.ok([...document.querySelectorAll('caption')].some(c => c.textContent === caption), caption);
    assert.match(document.querySelector('table').textContent, /Meera.*Library/);
    assert.match(text, /Llama 70B/);
  }, { url: `http://localhost/w/${w}/?view=settings&section=usage`, fetch: (path) => {
    if (path.endsWith('/session')) return Promise.resolve(response(session('Admin')));
    return new Promise(resolve => pending.push({ path, resolve }));
  } });
});

test('Members see their own use without a breakdown by person; an empty period says so', async () => {
  let empty = false;
  await reactFixture(async ({ render, change }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { UsagePage } = await import('../src/react/settings/usage-page.tsx');
    await render(UsagePage);
    assert.match(document.body.textContent, /Model use and charges for your requests/);
    assert.match(document.body.textContent, /The model and computer use made for your requests/);
    assert.equal([...document.querySelectorAll('caption')].some(c => c.textContent === 'By person'), false);
    empty = true;
    await change(document.querySelector('select'), '90');
    assert.match(document.body.textContent, /No model or computer use in the last 90 days/);
  }, { url: `http://localhost/w/${w}/?view=settings&section=usage`, fetch: async path => {
    if (path.endsWith('/session')) return response(session('Member'));
    const days = Number(new URL(path, 'http://x').searchParams.get('days'));
    return response(empty ? { ...report(days, 'mine'), totals: { calls: 0, input_tokens: 0, output_tokens: 0, charge_usd: 0 } } : report(days, 'mine'));
  } });
});

test('A charge too small for four decimals never reads as nothing', async () => {
  const { dollars } = await import('../src/react/usage/usage-model.ts');
  assert.equal(dollars(0.00002), 'under $0.0001');
  assert.equal(dollars(0), '$0.00');
  assert.equal(dollars(1.5), '$1.50');
  assert.equal(dollars(0.0042), '$0.0042');
});
