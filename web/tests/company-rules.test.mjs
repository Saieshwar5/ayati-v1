import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

const w = '11111111-1111-4111-8111-111111111111';
const base = `http://localhost/w/${w}/?view=settings&section=company-rules`;
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const actions = [{ connection: 'c1', account: 'Shop', operation: 'refund.create', title: 'Refund an order', effect: 'write', meaning: 'money',
  fields: [{ name: 'amount', type: 'number' }, { name: 'order', type: 'string' }, { name: 'reason', type: 'string' }] }];
const existing = { id: 'r1', operation: 'refund.create', connection: 'c1', meaning: null, kind: 'daily_total', field: 'amount', comparator: '>', value: 10000,
  effect: 'block', note: 'Finance limit', enabled: true, revision: 3, reviewed: true, summary: "Refund an order: if today's total amount would go over 10000, block it.", updated_by_name: 'Anita' };
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
    assert.deepEqual({ ...sent, id: undefined }, { id: undefined, operation: 'refund.create', connection: 'c1', meaning: null, kind: 'field', field: 'amount', comparator: '>', value: 2000, effect: 'ask', note: '', enabled: true, revision: 0, trial: false });
    assert.equal(posts[0].id, sent.id, 'a retry keeps the same rule identity');
    assert.ok(!document.querySelector('dialog[open]'), 'closed after saving');
    assert.match(document.body.textContent, /Company rule saved/);
    assert.match(document.body.textContent, /if amount is over 2000, ask first/);
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return session('Admin');
    if (path.endsWith('/guard/rules') && options.body) {
      const body = JSON.parse(options.body); posts.push(body);
      if (fail) return response({ error: 'No connected account offers this action' }, 400);
      const saved = { ...body, revision: 1, reviewed: true, summary: 'Refund an order: if amount is over 2000, ask first.' }; rules = [...rules, saved]; return response(saved);
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

test('One rule can cover every money action, and time rules read back their window and days', async () => {
  const posts = [];
  await reactFixture(async ({ render, button, click, change }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { SettingsPage } = await import('../src/react/settings/settings-page.tsx');
    await render(SettingsPage, { href: base });
    await click(button('Add rule'));
    await change(field('Applies to'), 'meaning:money');
    await click([...document.querySelectorAll('label')].find(l => l.textContent === "Today's total").querySelector('input'));
    await change(field('Value'), '5000');
    assert.equal(document.querySelector('.company-rule-readback').textContent, "Any money action: if today's total amount would go over 5000, ask first.");
    await click([...document.querySelectorAll('label')].find(l => l.textContent === 'Time of day').querySelector('input'));
    for (const day of ['Sun', 'Sat']) await click([...document.querySelectorAll('label')].find(l => l.textContent === day).querySelector('input'));
    assert.equal(document.querySelector('.company-rule-readback').textContent, 'Any money action: on Saturday and Sunday, between 21:00 and 09:00, ask first.');
    await click(button('Save rule'));
    const sent = posts[0];
    assert.deepEqual({ operation: sent.operation, connection: sent.connection, meaning: sent.meaning, kind: sent.kind, field: sent.field, comparator: sent.comparator, value: sent.value },
      { operation: null, connection: null, meaning: 'money', kind: 'time', field: '', comparator: '=', value: { from: '21:00', to: '09:00', days: [6, 7] } });
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return session('Admin');
    if (path.endsWith('/guard/rules') && options.body) { const body = JSON.parse(options.body); posts.push(body); return response({ ...body, summary: 'saved' }); }
    if (path.endsWith('/guard/rules')) return response({ rules: [], actions, can_manage: true });
    return response({});
  } });
});

test('A rule can use a value Cuckoo read earlier, or require that read first', async () => {
  const posts = [];
  const withRead = [...actions, { connection: 'c1', account: 'Shop', operation: 'order.read', title: 'Find an order', effect: 'read', meaning: 'read', fields: [{ name: 'order', type: 'string' }] }];
  await reactFixture(async ({ render, button, click, change }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { SettingsPage } = await import('../src/react/settings/settings-page.tsx');
    await render(SettingsPage, { href: base });
    await click(button('Add rule'));
    await change(field('Taken from'), 'order.read');
    assert.equal(field('Matched on').value, 'order');
    await change(field('Field in the result'), 'total');
    await change([...document.querySelectorAll('label')].find(n => n.firstChild?.textContent === 'Value').querySelector('input'), '2000');
    assert.equal(document.querySelector('.company-rule-readback').textContent, 'Refund an order: if total from Find an order is over 2000, ask first.');
    await click(button('Save rule'));
    assert.deepEqual({ kind: posts[0].kind, field: posts[0].field, value: posts[0].value, source: posts[0].source },
      { kind: 'field', field: 'total', value: 2000, source: { operation: 'order.read', match: 'order', path: 'total' } });
    await click(button('Add rule'));
    await click([...document.querySelectorAll('label')].find(l => l.textContent === 'Read first').querySelector('input'));
    assert.equal(document.querySelector('.company-rule-readback').textContent, 'Refund an order: unless Find an order for the same order ran first in this task, ask first.');
    await click(button('Save rule'));
    assert.deepEqual({ kind: posts[1].kind, field: posts[1].field, comparator: posts[1].comparator, value: posts[1].value, source: posts[1].source },
      { kind: 'read_first', field: '', comparator: '=', value: true, source: { operation: 'order.read', match: 'order' } });
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return session('Admin');
    if (path.endsWith('/guard/rules') && options.body) { const body = JSON.parse(options.body); posts.push(body); return response({ ...body, summary: 'saved' }); }
    if (path.endsWith('/guard/rules')) return response({ rules: [], actions: withRead, can_manage: true });
    return response({});
  } });
});

test('Admins review an agent proposal, try it on past requests, and turn it on with a trial week', async () => {
  const posts = [];
  const proposal = { id: 'p1', operation: 'refund.create', connection: 'c1', meaning: null, source: null, kind: 'field', field: 'amount', comparator: '>', value: 2000, effect: 'ask', note: '',
    enabled: false, revision: 1, reviewed: false, proposal: 'Refunds over 2000 need my approval', proposed_by_name: 'Nova', summary: 'Refund an order: if amount is over 2000, ask first.' };
  const replay = { days: 30, examined: 3, would_hold: 2, would_block: 0, summary: proposal.summary, examples: [{ id: 'a1', title: 'Refund an order', created: 1, state: 'completed', effect: 'ask', arguments: { order: 'R2', amount: 2500 }, day: 'Tuesday' }] };
  await reactFixture(async ({ render, button, click }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { SettingsPage } = await import('../src/react/settings/settings-page.tsx');
    await render(SettingsPage, { href: base });
    const section = document.querySelector('[aria-label="Proposed by agents"]');
    assert.match(section.textContent, /Nova proposed it from “Refunds over 2000 need my approval”\. Not in force\./);
    await click(button('Review'));
    assert.ok(document.querySelector('dialog[open]').textContent.includes('Review proposed rule'));
    await click(button('Try it on the last 30 days'));
    assert.match(document.querySelector('.company-rule-replay').textContent, /Of 3 matching requests, it would have held 2 and blocked 0\..*Ask · Refund an order · Tuesday/);
    assert.equal(posts.filter(p => p.path.endsWith('/replay')).length, 1);
    await click([...document.querySelectorAll('label')].find(l => l.textContent.startsWith('Start with a trial week')).querySelector('input'));
    await click(button('Turn on'));
    const saved = posts.find(p => p.path.endsWith('/guard/rules')).body;
    assert.deepEqual({ id: saved.id, enabled: saved.enabled, revision: saved.revision, trial: saved.trial }, { id: 'p1', enabled: true, revision: 1, trial: true });
    await click(button('Dismiss'));
    assert.ok(posts.some(p => p.path.endsWith('/guard/rules/p1/dismiss')));
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return session('Admin');
    if (options.body) { posts.push({ path, body: JSON.parse(options.body) }); return response(path.endsWith('/replay') ? replay : path.endsWith('/dismiss') ? { dismissed: true } : { ...proposal, reviewed: true, enabled: true }); }
    if (path.endsWith('/guard/rules')) return response({ rules: [proposal], actions, can_manage: true });
    return response({});
  } });
});

test('A rule takes its limit from a Library table: the number in the row the request names', async () => {
  const posts = [];
  const discount = [{ connection: 'c1', account: 'Shop', operation: 'discount.apply', title: 'Apply a discount', effect: 'write', meaning: 'money',
    fields: [{ name: 'discount', type: 'number' }, { name: 'product', type: 'string' }] }];
  await reactFixture(async ({ render, button, click, change }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { SettingsPage } = await import('../src/react/settings/settings-page.tsx');
    await render(SettingsPage, { href: base });
    await click(button('Add rule'));
    await change(field('Compare with'), 'table');
    assert.equal(field('Value'), undefined, 'no fixed value');
    assert.equal(field('Column').value, 'Max discount');
    assert.equal(field('Row named by').value, 'product');
    assert.equal(document.querySelector('.company-rule-readback').textContent, 'Apply a discount: if discount is over Max discount in Price list for its product, ask first.');
    await change(field('Comparison'), '=');
    assert.equal(field('Compare with'), undefined, 'equality compares with a fixed value');
    await change(field('Comparison'), '>');
    await click(button('Save rule'));
    assert.deepEqual(posts[0].value, { table: 't1', title: 'Price list', column: 'Max discount', match: 'product' });
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return session('Admin');
    if (options.body) { posts.push(JSON.parse(options.body)); return response({ ...JSON.parse(options.body), summary: 'saved', reviewed: true }); }
    return response({ rules: [], actions: discount, can_manage: true, tables: [{ id: 't1', title: 'Price list', columns: ['Max discount', 'Price'] }, { id: 't2', title: 'Names', columns: [] }] });
  } });
});
