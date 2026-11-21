// Activity → Needs me in a wide window (designs/activity-next, Design A): the opened item
// shows beside the list instead of in a dialog over it.
import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=activity&tab=needs`;
const session = { user: { id: 'meera', email: 'meera@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const now = Date.now();
const base = { version: 'pending', status: 'pending', at: now, agent: 'nova', agent_name: 'Nova', chat: 'sales', thread: null, conversation_name: 'sales', archived: false, read: false, reviewed: false };
const approval = { ...base, kind: 'approval', id: 'a1', title: 'Discount for Sharma Traders', reply: 'r-a1', created_at: now - 6 * 3600000, backup_at: now - 3600000 };
const question = { ...base, kind: 'question', id: 'q1', title: 'Which courier?', reply: 'r-q1' };
const wide = () => { window.matchMedia = () => ({ matches: true, addEventListener() {}, removeEventListener() {} }); };
async function page() {
  const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
  return (await import('../src/react/activity/activity-page.tsx')).ActivityPage;
}
const rowOf = title => [...document.querySelectorAll('.activity-need-row')].find(node => node.textContent.includes(title));
const press = key => document.body.dispatchEvent(new window.KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
/** A small server: the list, one approval, one question, receipts and decisions. */
function server({ denyReceipt = false } = {}) {
  const state = { receipts: [], decisions: [], lists: 0, decided: false };
  const fetch = async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/attention') && options.method === 'POST') {
      if (denyReceipt) return response({ error: 'You can no longer see this request.' }, 403);
      state.receipts.push(JSON.parse(options.body).id); return response({ saved: true });
    }
    if (path.includes('/attention')) { state.lists += 1; return response({ available: true, next_before: null, items: state.decided ? [question] : [approval, question] }); }
    if (path.endsWith('/actions/a1/decision')) { state.decisions.push(JSON.parse(options.body).decision); state.decided = true; return response({ id: 'a1', state: 'approved' }); }
    if (path.endsWith('/actions/a1')) return response({ id: 'a1', title: 'Create quote', arguments: { customer: 'Sharma Traders', discount: '12%' }, agent: 'nova', agent_name: 'Nova', account_name: 'Zoho Books',
      conversation_name: 'sales', requester_name: 'Kavya Nair', chat: 'sales', thread: null, can_decide: !state.decided, state: state.decided ? 'approved' : 'pending',
      decision: state.decided ? 'approve' : null, decided_by_name: state.decided ? 'Meera Iyer' : null });
    if (path.includes('/questions/')) return response({ id: 'q1', revision: 1, state: 'pending', specification: { version: 1, kind: 'single_choice', title: 'Pick a courier', description: '', fields: [], options: [{ id: 'a', label: 'BlueDart' }] },
      respondent: 'meera', respondent_name: 'Meera', can_respond: true, reply: 'r-q1', chat: 'sales' });
    return response({});
  };
  return { state, fetch };
}

test('a wide window opens an item beside the list after its read receipt, re-reads the list after a decision and offers the next item', async () => {
  const { state, fetch } = server();
  await reactFixture(async ({ render, click, button }) => {
    wide();
    await render(await page(), { href: url });
    assert.ok(document.querySelector('.activity-page.activity-split'), 'the page uses the two-part layout');
    assert.equal(document.querySelector('.ui-pane'), null, 'nothing is opened, so nothing is marked read');
    assert.deepEqual(state.receipts, []);
    assert.match(rowOf('Discount for Sharma Traders').textContent, /An admin can decide now/, 'urgency is a label, not grey text');

    await click(rowOf('Discount for Sharma Traders'));
    assert.deepEqual(state.receipts, ['a1'], 'the read receipt is saved first');
    assert.equal(document.querySelector('dialog'), null, 'no dialog covers the list');
    const pane = document.querySelector('.ui-pane');
    assert.match(pane.textContent, /Create quote.*Sharma Traders/s);
    assert.equal(rowOf('Discount for Sharma Traders').getAttribute('aria-current'), 'true');
    assert.ok(rowOf('Which courier?'), 'the list stays in view');

    const lists = state.lists;
    await click(button('Approve this action'));
    assert.deepEqual(state.decisions, ['approve']);
    assert.ok(state.lists > lists, 'the list is read again once the decision is saved');
    assert.equal(rowOf('Discount for Sharma Traders'), undefined, 'the decided item leaves the list');
    assert.match(document.querySelector('.ui-pane').textContent, /Approved by Meera Iyer/, 'the pane keeps showing what was decided');

    await click(button('Next: Which courier?'));
    assert.deepEqual(state.receipts, ['a1', 'q1'], 'the next item is opened only when asked, with its own receipt');
    assert.match(document.querySelector('.ui-pane h2').textContent, /Pick a courier/);
  }, { url, fetch });
});

test('J and K open the next and previous row with one receipt each, and do nothing while typing', async () => {
  const { state, fetch } = server();
  await reactFixture(async ({ render, update }) => {
    wide();
    await render(await page(), { href: url });
    await update(() => press('j'));
    assert.deepEqual(state.receipts, ['a1']);
    assert.equal(document.activeElement, rowOf('Discount for Sharma Traders'), 'focus follows the opened row');
    await update(() => press('j'));
    assert.deepEqual(state.receipts, ['a1', 'q1']);
    await update(() => press('j'));
    assert.deepEqual(state.receipts, ['a1', 'q1'], 'the last row stays open');
    await update(() => press('k'));
    assert.deepEqual(state.receipts, ['a1', 'q1', 'a1']);
    const search = document.querySelector('.activity-search');
    await update(() => { search.focus(); search.dispatchEvent(new window.KeyboardEvent('keydown', { key: 'j', bubbles: true, cancelable: true })); });
    assert.equal(state.receipts.length, 3, 'a key typed into a field is only text');
  }, { url, fetch });
});

test('a failed read receipt shows the error and opens nothing beside the list', async () => {
  const { state, fetch } = server({ denyReceipt: true });
  await reactFixture(async ({ render, click }) => {
    wide();
    await render(await page(), { href: url });
    await click(rowOf('Which courier?'));
    assert.deepEqual(state.receipts, []);
    assert.match(document.querySelector('[role=alert]').textContent, /no longer see this request/);
    assert.equal(document.querySelector('.ui-pane'), null);
  }, { url, fetch });
});

test('a failed outcome is cleared from beside its details, with one receipt for a double click', async () => {
  const failed = { ...base, kind: 'outcome', id: 'o1', version: 'failed:1', status: 'failed', title: 'Monthly report', reply: 'r-o1' };
  const receipts = []; let reviewed = false;
  await reactFixture(async ({ render, click, update, button }) => {
    wide();
    await render(await page(), { href: url });
    assert.equal(button('Mark reviewed'), undefined, 'rows have no buttons of their own');
    await click(rowOf('Monthly report'));
    await update(() => { const node = button('Mark reviewed'); node.click(); node.click(); });
    assert.deepEqual(receipts, [{ id: 'o1', reviewed: false }, { id: 'o1', reviewed: true }]);
    assert.match(document.body.textContent, /Nothing needs you right now/);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/attention') && options.method === 'POST') { const body = JSON.parse(options.body); receipts.push({ id: body.id, reviewed: body.reviewed }); reviewed ||= body.reviewed; return response({ saved: true }); }
    if (path.includes('/attention')) return response({ available: true, next_before: null, items: reviewed ? [] : [failed] });
    if (path.includes('/activity/requests/')) return response({ id: 'r-o1', chat: 'sales', thread: null, conversation_name: 'sales', conversation_kind: 'channel', in_conversation: true, agent: 'nova', agent_name: 'Nova', agent_avatar: null,
      kind: 'chat', requester: 'kavya', requester_name: 'Kavya Nair', summary: 'Monthly report', state: 'failed', latest_update: '', created_ms: now, finished_ms: now + 1000, can_cancel: false, approvals: [], message: 'Run the report', reply: 'I could not.', steps: [], fix: null, disagreement: null });
    return response({});
  } });
});

test('a narrow window keeps today\'s dialog and its named button on every row', async () => {
  const { state, fetch } = server();
  await reactFixture(async ({ render, click }) => {
    await render(await page(), { href: url });
    assert.equal(document.querySelector('.activity-split'), null);
    assert.equal(document.querySelector('.activity-need-row'), null);
    await click([...document.querySelectorAll('button:not([role=tab])')].find(node => node.textContent === 'Review'));
    assert.deepEqual(state.receipts, ['a1']);
    assert.match(document.querySelector('dialog.ui-drawer').textContent, /Create quote/);
  }, { url, fetch });
});
