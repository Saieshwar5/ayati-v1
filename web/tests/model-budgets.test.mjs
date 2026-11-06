// MG3: monthly model budgets. Everyone sees what is left; only admins change them; a
// double click saves once and a failed save keeps the draft.
import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const budget = (agent, name, used, monthly, state) => ({ agent, name, used_usd: used, monthly_usd: monthly, percent: Math.floor(used * 100 / monthly), warn_percent: 80, state, month_start_ms: 0 });
const budgets = can_change => ({ can_change, budgets: [budget('', 'Workspace', 42, 50, 'warning'), budget('a', 'Meera', 10, 10, 'reached')] });
const session = role => ({ user: { id: 'p', email: 'p@test' }, workspaces: [{ id: w, name: 'Store', role }] });
const directory = { agents: [{ id: 'a', name: 'Meera' }, { id: 'b', name: 'Tara' }], role: 'Admin', default_model: 'm', providers: [], system_prompt: '' };

test('Admins set a budget once per click, keep the draft after a failure, and remove budgets', async () => {
  const writes = []; let complete;
  await reactFixture(async ({ render, button, change, click, update }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { BudgetsView } = await import('../src/react/usage/budgets.tsx');
    await render(BudgetsView);
    const text = document.body.textContent;
    assert.match(text, /Workspace\$42\.00 of \$50\.00 this month/);
    assert.match(text, /Past 80% of the budget/);
    assert.match(text, /Meera.*Used up: this agent takes no new requests/);
    assert.equal(document.querySelector('meter[aria-label^="Meera"]').value, 100);
    const form = document.querySelector('form[aria-label="Set a monthly budget"]');
    await change(form.querySelector('select'), 'b');
    await change(form.querySelector('input'), '25');
    await update(() => { const save = button('Set budget'); save.click(); save.click(); });
    assert.equal(writes.length, 1, 'one save for a double click');
    assert.deepEqual(writes[0], { agent: 'b', monthly_usd: 25, warn_percent: 80 });
    await update(() => complete(response({ error: 'Save failed' }, 503)));
    assert.match(document.body.textContent, /Save failed/);
    assert.equal(form.querySelector('input').value, '25', 'the draft stays');
    await click(button('Set budget'));
    await update(() => complete(response({ can_change: true, budgets: [...budgets(true).budgets, budget('b', 'Tara', 0, 25, 'ok')] })));
    assert.match(document.body.textContent, /Budget saved/);
    assert.match(document.body.textContent, /Tara\$0\.00 of \$25\.00/);
    await click(button('Remove the budget for Meera'));
    assert.deepEqual(writes.at(-1), { agent: 'a', monthly_usd: null, warn_percent: 80 });
  }, { url: `http://localhost/w/${w}/?view=settings&section=usage`, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session('Admin'));
    if (options.method === 'POST') { writes.push(JSON.parse(options.body)); return new Promise(resolve => { complete = resolve; }); }
    if (path.endsWith('/agent-profiles')) return response(directory);
    return response(budgets(true));
  } });
});

test('Members see budgets without controls', async () => {
  await reactFixture(async ({ render }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { BudgetsView } = await import('../src/react/usage/budgets.tsx');
    await render(BudgetsView);
    assert.match(document.body.textContent, /Workspace\$42\.00 of \$50\.00/);
    assert.equal(document.querySelector('form'), null);
    assert.equal([...document.querySelectorAll('button')].some(b => b.textContent === 'Remove'), false);
  }, { url: `http://localhost/w/${w}/?view=settings&section=usage`, fetch: async path => response(path.endsWith('/session') ? session('Member') : budgets(false)) });
});
