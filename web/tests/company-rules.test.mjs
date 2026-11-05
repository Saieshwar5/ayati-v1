import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

const w = '11111111-1111-4111-8111-111111111111';
const base = `http://localhost/w/${w}/?view=settings&section=company-rules`;
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const actions = [{ connection: 'c1', account: 'Shop', operation: 'refund.create', title: 'Refund an order', effect: 'write',
  fields: [{ name: 'amount', type: 'number' }, { name: 'order', type: 'string' }, { name: 'reason', type: 'string' }] }];
const existing = { id: 'r1', operation: 'refund.create', connection: 'c1', kind: 'daily_total', field: 'amount', comparator: '>', value: 10000,
  effect: 'block', note: 'Finance limit', enabled: true, revision: 3, summary: "Refund an order: if today's total amount would go over 10000, block it.", updated_by_name: 'Anita' };
const field = label => [...document.querySelectorAll('label')].find(node => node.textContent.startsWith(label))?.querySelector('input,select');
const session = role => response({ user: { id: 'me', email: 'me@test' }, workspaces: [{ id: w, name: 'Mehta', role }] });

test('Admins add a rule with a plain-words read-back, saving once and keeping the draft after a failure', async () => {
  const posts = []; let fail = true; let rules = [existing];
  await reactFixture(async ({ render, button, click, change, update }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { SettingsPage } = await import('../src/react/settings/settings-page.tsx');
    await render(SettingsPage, { href: base });
    assert.match(document.body.textContent, /would go over 10000, block it\..*Finance limit · Changed by Anita/);
    await click(button('Add rule'));
    assert.equal(field('Field').value, 'amount', 'amounts first');
    await change(field('Value'), '2000');
    assert.equal(document.querySelector('.company-rule-readback').textContent, 'Refund an order: if amount is over 2000, ask first.');
    await update(() => { button('Save rule').click(); button('Save rule').click(); });
    assert.equal(posts.length, 1, 'one request per click burst');
    assert.match(document.body.textContent, /No connected account/);
    assert.equal(field('Value').value, '2000', 'the draft stays');
    fail = false;
    await click(button('Save rule'));
    const sent = posts.at(-1);
    assert.deepEqual({ ...sent, id: undefined }, { id: undefined, operation: 'refund.create', connection: 'c1', kind: 'field', field: 'amount', comparator: '>', value: 2000, effect: 'ask', note: '', enabled: true, revision: 0 });
    assert.equal(posts[0].id, sent.id, 'a retry keeps the same rule identity');
    assert.ok(!document.querySelector('dialog[open]'), 'closed after saving');
    assert.match(document.body.textContent, /Company rule saved/);
    assert.match(document.body.textContent, /if amount is over 2000, ask first/);
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return session('Admin');
    if (path.endsWith('/guard/rules') && options.body) {
      const body = JSON.parse(options.body); posts.push(body);
      if (fail) return response({ error: 'No connected account offers this action' }, 400);
      const saved = { ...body, revision: 1, summary: 'Refund an order: if amount is over 2000, ask first.' }; rules = [...rules, saved]; return response(saved);
    }
    if (path.endsWith('/guard/rules')) return response({ rules, actions, can_manage: true });
    return response({});
  } });
});

test('Text fields only offer is / is not, totals only amounts, and turning off keeps the revision', async () => {
  const posts = [];
  await reactFixture(async ({ render, button, click, change }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { SettingsPage } = await import('../src/react/settings/settings-page.tsx');
    await render(SettingsPage, { href: base });
    await click(button('Add rule'));
    await change(field('Field'), 'reason');
    assert.deepEqual([...field('Comparison').options].map(o => o.textContent), ['is', 'is not']);
    await change(field('Value'), 'fraud');
    await click([...document.querySelectorAll('label')].find(l => l.textContent === 'Block').querySelector('input'));
    assert.equal(document.querySelector('.company-rule-readback').textContent, 'Refund an order: if reason is "fraud", block it.');
    await click([...document.querySelectorAll('label')].find(l => l.textContent === "Today's total").querySelector('input'));
    assert.deepEqual([...field('Field').options].map(o => o.value), ['amount'], 'totals add up numbers only');
    await click(button('Cancel'));
    await click(button('Turn off'));
    assert.deepEqual({ enabled: posts[0].enabled, revision: posts[0].revision, id: posts[0].id }, { enabled: false, revision: 3, id: 'r1' });
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return session('Admin');
    if (path.endsWith('/guard/rules') && options.body) { const body = JSON.parse(options.body); posts.push(body); return response({ ...body, summary: existing.summary }); }
    if (path.endsWith('/guard/rules')) return response({ rules: [existing], actions, can_manage: true });
    return response({});
  } });
});

test('Members read company rules but cannot change them', async () => {
  await reactFixture(async ({ render, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { SettingsPage } = await import('../src/react/settings/settings-page.tsx');
    await render(SettingsPage, { href: base });
    assert.match(document.body.textContent, /would go over 10000, block it/);
    assert.equal(button('Add rule'), undefined);
    assert.equal(button('Edit'), undefined);
    assert.ok([...document.querySelectorAll('.settings-sidebar button')].some(b => b.textContent === 'Company rules'));
  }, { url: base, fetch: async path => path.endsWith('/session') ? session('Member') : response({ rules: [existing], actions, can_manage: false }) });
});

test('An approval held by a company rule says which rule and the facts it used', async () => {
  const held = { id: 'a1', title: 'Refund an order', agent: 'ag', agent_name: 'Atlas', account_name: 'Shop', conversation_name: 'Orders', requester_name: 'Ravi',
    state: 'pending', arguments: { order: 'A-2', amount: 2500 }, chat: 'c', thread: null, can_decide: true, deciders: [{ id: 'me', name: 'Anita' }],
    guard: { rule: 'r2', effect: 'ask', known: true, matched: 1, summary: 'Refund an order: if amount is over 2000, ask first.', facts: { field: 'amount', value: 2500 } } };
  await reactFixture(async ({ render }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { ApprovalDialog } = await import('../src/react/approvals/approval-dialog.tsx');
    await render(ApprovalDialog, { id: 'a1', close: () => {} });
    const note = document.querySelector('.approval-guard');
    assert.match(note.textContent, /Held by a company rule.*if amount is over 2000, ask first\..*This request's amount: 2500\./);
  }, { url: `http://localhost/w/${w}/`, fetch: async path => path.endsWith('/session') ? session('Admin') : response(held) });
});
