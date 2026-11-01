import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

// Home panel refreshes in History: their own type, and one line per panel per day.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=activity&tab=history`;
const session = { user: { id: 'kavya', email: 'kavya@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Member' }] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
// Midday today, so every sample falls on the same day whenever the test runs.
const now = (() => { const d = new Date(); d.setHours(12, 0, 0, 0); return d.getTime(); })();
const totals = { requests: 5, done: 4, failed: 1, review: 1, waiting: 0, automatic: 0, approved: 0, people: 1, channels: 1, schedule_runs: 0, per_day: [], by_agent: [] };
const item = (id, extra = {}) => ({ id, chat: 'sales', thread: null, conversation_name: 'sales', conversation_kind: 'channel', in_conversation: true,
  agent: 'nova', agent_name: 'Nova', agent_avatar: null, kind: 'chat', requester: 'kavya', requester_name: 'Kavya Nair', summary: 'Quote for Sharma Traders',
  state: 'complete', latest_update: '', created_ms: now, finished_ms: now + 1000, can_cancel: false, approvals: [], ...extra });
const panel = (id, minutes, extra = {}) => item(id, { kind: 'panel', panel: { id: 'sales-panel', title: 'Sales today' }, summary: 'Updated panel: Sales today',
  created_ms: now - minutes * 60000, ...extra });
const page = [panel('p1', 1), item('c1', { created_ms: now - 2 * 60000 }), panel('p2', 61), panel('p3', 121, { state: 'failed' }), panel('p4', 181)];

test('routine updates fold into one line per panel per day; a failed one stays on its own', async () => {
  const { foldPanelUpdates } = await import('../src/react/activity/model.ts');
  const entries = foldPanelUpdates(page);
  assert.deepEqual(entries.map(e => e.kind === 'item' ? e.item.id : e.items.map(i => i.id).join('+')), ['p1+p2+p4', 'c1', 'p3']);
  assert.deepEqual(foldPanelUpdates([panel('only', 1)]).map(e => e.kind), ['item'], 'a single update is just a row');
});

test('History shows the folded line, expands it, and the Type filter asks for panel updates', async () => {
  const reads = [];
  await reactFixture(async ({ render, button, click, change }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { HistoryView } = await import('../src/react/activity/history.tsx');
    await render(HistoryView, { search: '' });
    const fold = [...document.querySelectorAll('.activity-row-button')].find(node => /updated \d times/.test(node.textContent));
    assert.ok(fold, 'a folded line');
    assert.match(fold.textContent, /Updated panel: Sales today/);
    assert.equal(fold.getAttribute('aria-expanded'), 'false');
    assert.match(document.body.textContent, /Could not complete/, 'the failed update keeps its own line');
    await click(fold);
    assert.equal(fold.getAttribute('aria-expanded'), 'true');
    assert.ok(document.querySelectorAll('.activity-fold .activity-row-button').length >= 2);
    const type = [...document.querySelectorAll('label')].find(node => node.textContent.startsWith('Type')).querySelector('select');
    await change(type, 'panel');
    assert.match(reads.filter(path => path.includes('/activity/history')).at(-1), /kind=panel/);
  }, { url, fetch: async path => {
    reads.push(path);
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/activity/totals')) return response(totals);
    if (path.endsWith('/agent-profiles')) return response({ agents: [], role: 'Member' });
    if (path.endsWith('/chat-directory')) return response({ users: [], agents: [] });
    if (path.endsWith('/chats')) return response({ chats: [] });
    return response({ items: page, next_before: null });
  } });
});
