// MG2: model use and charges by agent, model, person and day. A slow answer for an
// old period never replaces the current one; members see only their own use.
import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
// The report's days are the workspace's; the chart ends on today, so the fixture's day is today.
const today = (() => { const now = new Date(); return new Date(now.getTime() - now.getTimezoneOffset() * 60000).toISOString().slice(0, 10); })();
const row = (key, name, calls, charge) => ({ key, name, calls, input_tokens: calls * 1000, output_tokens: calls * 200, charge_usd: charge });
const report = (days, scope = 'workspace') => ({ days, since_ms: 0, scope, totals: { calls: 12, input_tokens: 12000, output_tokens: 2400, charge_usd: days === 7 ? 0.0042 : 1.5 },
  by_agent: [row('a', 'Meera', 10, 1.2), row('', 'Library', 2, 0.3)], by_model: [row('llama-70b', 'Llama 70B', 12, 1.5)],
  by_person: [row('p', 'Ananya', 12, 1.5)], by_day: [row(today, undefined, 12, 1.5)] });
const session = role => ({ user: { id: 'p', email: 'p@test' }, workspaces: [{ id: w, name: 'Store', role }] });

test('Usage shows charges by agent, model, person and day, and a stale period never wins', async () => {
  const pending = [];
  await reactFixture(async ({ render, change, update, click }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { UsageReportView } = await import('../src/react/usage/usage-report.tsx');
    await render(UsageReportView, { title: 'Workspace usage' });
    const thirty = pending.find(p => p.path.includes('days=30'));
    await change(document.querySelector('select'), '7');
    const seven = pending.find(p => p.path.includes('days=7'));
    await update(() => seven.resolve(response(report(7))));
    await update(() => thirty.resolve(response(report(30))));
    assert.equal(document.querySelector('[aria-label=Charges] strong').textContent, '$0.0042', 'the 7-day answer stays after the slow 30-day one');
    assert.match(document.querySelector('[aria-label=Charges]').textContent, /Charges, last 7 days.*12 model requests/);
    // One table, grouped the way you choose (2026-11-21; it was four tables).
    const groups = () => [...document.querySelectorAll('.usage-groups button')];
    assert.deepEqual(groups().map(node => [node.textContent, node.getAttribute('aria-pressed')]), [['By agent', 'true'], ['By model', 'false'], ['By person', 'false'], ['By day', 'false']]);
    assert.equal(document.querySelectorAll('table').length, 1);
    const cells = () => [...document.querySelectorAll('tbody tr')].map(line => [...line.children].map(cell => cell.textContent));
    assert.deepEqual(cells(), [['Meera', '10', '$1.20', '80%'], ['Library', '2', '$0.30', '20%']]);
    await click(groups()[1]);
    assert.equal(document.querySelector('caption').textContent, 'By model');
    assert.deepEqual(cells(), [['Llama 70B', '12', '$1.50', '100%']]);
    // Token counts are there when asked for.
    assert.deepEqual([...document.querySelectorAll('thead th')].map(node => node.textContent), ['Model', 'Requests', 'Charges', 'Share']);
    await click(document.querySelector('.usage-tokens input'));
    assert.deepEqual(cells(), [['Llama 70B', '12', '12,000', '2,400', '$1.50', '100%']]);
    // A bar for every day of the period; the day with use names itself.
    assert.equal(document.querySelectorAll('.usage-chart-bars > span').length, 7);
    assert.match(document.querySelector('.usage-chart-bars').getAttribute('aria-label'), /over 7 days\. The highest was \$1\.50 on /);
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
    assert.deepEqual([...document.querySelectorAll('.usage-groups button')].map(node => node.textContent), ['By agent', 'By model', 'By day']);
    empty = true;
    await change(document.querySelector('select'), '90');
    assert.match(document.body.textContent, /No model or computer use in the last 90 days/);
  }, { url: `http://localhost/w/${w}/?view=settings&section=usage`, fetch: async path => {
    if (path.endsWith('/session')) return response(session('Member'));
    const days = Number(new URL(path, 'http://x').searchParams.get('days'));
    return response(empty ? { ...report(days, 'mine'), totals: { calls: 0, input_tokens: 0, output_tokens: 0, charge_usd: 0 } } : report(days, 'mine'));
  } });
});

test('The chart has a bar for every day of the period, with nothing on days without use', async () => {
  const { chartDays } = await import('../src/react/usage/usage-chart.tsx');
  const days = chartDays([row('2026-11-03', undefined, 1, 0.5), row('2026-11-05', undefined, 2, 1.5)], 4, new Date(2026, 10, 5, 12));
  assert.deepEqual(days, [{ key: '2026-11-02', charge: 0 }, { key: '2026-11-03', charge: 0.5 }, { key: '2026-11-04', charge: 0 }, { key: '2026-11-05', charge: 1.5 }]);
  // The workspace's day can be ahead of this device's: the newest reported day ends the range.
  assert.equal(chartDays([row('2026-11-06', undefined, 1, 1)], 2, new Date(2026, 10, 5, 23)).at(-1).key, '2026-11-06');
});

test('A charge too small for four decimals never reads as nothing', async () => {
  const { dollars } = await import('../src/react/usage/usage-model.ts');
  assert.equal(dollars(0.00002), 'under $0.0001');
  assert.equal(dollars(0), '$0.00');
  assert.equal(dollars(1.5), '$1.50');
  assert.equal(dollars(0.0042), '$0.0042');
});
