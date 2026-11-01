import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=activity&tab=history`;
const session = { user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const totals = { requests: 2, done: 1, failed: 1, review: 1, waiting: 0, automatic: 3, approved: 1, people: 1, channels: 1, schedule_runs: 0, per_day: [], by_agent: [] };
const now = Date.now();
const item = (id, summary, extra = {}) => ({ id, chat: 'sales', thread: null, conversation_name: 'sales', conversation_kind: 'channel', in_conversation: true,
  agent: 'nova', agent_name: 'Nova', agent_avatar: null, kind: 'chat', requester: 'owner', requester_name: 'Kavya Nair', summary, state: 'complete',
  latest_update: '', created_ms: now, finished_ms: now + 2000, can_cancel: false, approvals: [], ...extra });
const common = path => {
  if (path.endsWith('/session')) return response(session);
  if (path.endsWith('/agent-profiles')) return response({ agents: [{ id: 'nova', name: 'Nova' }], role: 'Admin' });
  if (path.endsWith('/chat-directory')) return response({ users: [{ id: 'owner', name: 'Kavya Nair' }], agents: [] });
  if (path.endsWith('/chats')) return response({ chats: [{ id: 'sales', name: 'sales', kind: 'channel', users: [], agents: [] }] });
  if (path.includes('/attention')) return response({ available: true, items: [], next_before: null });
  if (path.includes('/activity/totals')) return response(totals);
};
async function page() {
  const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
  return (await import('../src/react/activity/activity-page.tsx')).ActivityPage;
}
const select = label => [...document.querySelectorAll('label')].find(node => node.textContent.startsWith(label))?.querySelector('select');

test('History asks the server for each filter, pages without repeats and drops late pages for old filters', async () => {
  const reads = []; let late;
  await reactFixture(async ({ render, button, click, change, update }) => {
    await render(await page(), { href: url });
    assert.match(document.body.textContent, /Quote for Sharma Traders/);
    assert.match(document.querySelector('[aria-label="Needs review"]').textContent, /1/);
    await click(button('Show older'));
    assert.match(document.body.textContent, /Older request/);
    assert.equal(document.querySelectorAll('.activity-row').length, 2);
    // A slow page for the old filters arrives after the filters change.
    const slow = button('Show older'); assert.equal(slow, undefined);
    await change(select('Status'), 'failed');
    const filtered = reads.filter(path => path.includes('/activity/history')).at(-1);
    assert.match(filtered, /status=failed/); assert.match(filtered, /from=\d+/);
    assert.match(document.body.textContent, /Status: Could not complete/);
    await click(button('Remove Status: Could not complete'));
    assert.equal(reads.filter(path => path.includes('/activity/history')).at(-1).includes('status='), false);
    await change(select('Agent'), 'nova');
    late?.(response({ items: [item('stale', 'STALE_OLD_FILTER')], next_before: null }));
    await update();
    assert.equal(document.body.textContent.includes('STALE_OLD_FILTER'), false);
  }, { url, fetch: async path => {
    reads.push(path);
    const shared = common(path); if (shared) return shared;
    if (path.includes('agent=nova') && !late) return new Promise(resolve => { late = resolve; });
    if (path.includes('before=')) return response({ items: [item('older', 'Older request')], next_before: null });
    return response({ items: [item('first', 'Quote for Sharma Traders')], next_before: 10 });
  } });
});

test('request details show each step, a fix, and a warning when the reply claims what the records do not', async () => {
  await reactFixture(async ({ render, button, click }) => {
    await render(await page(), { href: url });
    const row = document.querySelector('.activity-row-button');
    assert.match(row.textContent, /Nova“Quote for Sharma Traders”Kavya Nair · #salesDone/);
    await click(row);
    const drawer = document.querySelector('dialog');
    assert.match(drawer.textContent, /Zoho Books: Create quote/);
    assert.match(drawer.textContent, /Approved by Meera Iyer/);
    assert.match(drawer.textContent, /no record of a change/);
    assert.match(drawer.textContent, /How to fix it.*Upload the price list/);
    assert.match(drawer.querySelector('a').href, /conversation=sales&message=first/);
  }, { url, fetch: async path => {
    const shared = common(path); if (shared) return shared;
    if (path.includes('/activity/requests/first')) return response({ ...item('first', 'Quote for Sharma Traders'), message: 'Quote for Sharma Traders', reply: 'I sent the quote.',
      steps: [{ id: 's1', title: 'Create quote', effect: 'write', state: 'completed', automatic: false, decision: 'approve', decided_by: 'Meera Iyer', account: 'Zoho Books', arguments: { Customer: 'Sharma Traders' }, at: 1 }],
      fix: 'Upload the price list, then ask again.', disagreement: 'The reply says something was sent, but there is no record of a change.' });
    return response({ items: [item('first', 'Quote for Sharma Traders')], next_before: null });
  } });
});

test('Now never records a visit by itself; Dismiss records it once and hides the summary', async () => {
  const seen = []; let caughtUp = false;
  await reactFixture(async ({ render, button, update }) => {
    await render(await page(), { href: `http://localhost/w/${w}/?view=activity&tab=now` });
    assert.equal(seen.length, 0);
    assert.match(document.querySelector('[aria-label="While you were away"]').textContent, /2 requests · 1 done · 1 failed/);
    assert.match(document.querySelector('[aria-label="Nova"]').textContent, /Working/);
    assert.match(document.body.textContent, /Running now · 1/);
    await update(() => { const node = button('Dismiss'); node.click(); node.click(); });
    assert.equal(seen.length, 1);
    assert.equal(document.querySelector('[aria-label="While you were away"]'), null);
  }, { url, fetch: async (path, options = {}) => {
    const shared = common(path); if (shared) return shared;
    if (path.endsWith('/activity/seen')) { seen.push(options.method); caughtUp = true; return response({ since: now }); }
    if (path.includes('/activity/now')) return response({
      agents: [{ id: 'nova', name: 'Nova', role: 'Sales follow-up', avatar: null, paused: false, paused_note: null, running: 1, waiting: 0, today: 2, failed_today: 0, doing: 'Quote for Sharma Traders', next_run: null }],
      running: [item('run', 'Quote for Sharma Traders', { state: 'running', can_cancel: true })],
      away: { since: now - 3600000, totals: caughtUp ? { ...totals, requests: 0 } : totals } });
  } });
});

test("an agent's Activity tab is History for that agent only", async () => {
  const reads = [];
  await reactFixture(async ({ render }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { HistoryView } = await import('../src/react/activity/history.tsx');
    await render(HistoryView, { agent: 'nova', search: '' });
    assert.equal(select('Agent'), undefined);
    assert.ok(reads.filter(path => path.includes('/activity/')).every(path => path.includes('agent=nova')));
  }, { url, fetch: async path => {
    reads.push(path);
    const shared = common(path); if (shared) return shared;
    return response({ items: [], next_before: null });
  } });
});

test('Stop in request details sends one cancel for a double click and then shows the stopped request', async () => {
  const writes = []; let stopped = false;
  await reactFixture(async ({ render, button, update }) => {
    await render(await page(), { href: url });
    await update(() => document.querySelector('.activity-row-button').click());
    await update(() => { const node = button('Stop request'); node.click(); node.click(); });
    assert.equal(writes.length, 1);
    assert.ok(writes[0].endsWith('/chats/sales/messages/first/cancel'));
    assert.match(document.querySelector('dialog').textContent, /Stopped/);
    assert.equal(button('Stop request'), undefined);
  }, { url, fetch: async (path, options = {}) => {
    const shared = common(path); if (shared) return shared;
    if (options.method === 'POST') { writes.push(path); stopped = true; return response({ ok: true }); }
    const current = item('first', 'Quote for Sharma Traders', stopped ? { state: 'cancelled' } : { state: 'running', can_cancel: true });
    if (path.includes('/activity/requests/first')) return response({ ...current, message: 'Quote', reply: '', steps: [], fix: null, disagreement: null });
    return response({ items: [current], next_before: null });
  } });
});

test('charts fill empty days, label the busiest one, keep a table, and Export carries the filters', async () => {
  const day = offset => { const d = new Date(); d.setHours(0, 0, 0, 0); d.setDate(d.getDate() - offset); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`; };
  const charted = { ...totals, per_day: [{ day: day(0), count: 4 }, { day: day(3), count: 9 }], by_agent: [{ agent: 'nova', name: 'Nova', avatar: null, count: 9 }, { agent: 'atlas', name: 'Atlas', avatar: null, count: 4 }] };
  await reactFixture(async ({ render, change }) => {
    await render(await page(), { href: url });
    const columns = [...document.querySelectorAll('.chart-columns > li')];
    assert.equal(columns.length, 7, 'last 7 days, empty days included');
    assert.deepEqual(columns.map(c => c.getAttribute('aria-label').split(': ')[1]), ['0 requests', '0 requests', '0 requests', '9 requests', '0 requests', '0 requests', '4 requests']);
    assert.deepEqual([...document.querySelectorAll('.chart-value')].map(v => v.textContent), ['9'], 'only the peak is labelled');
    assert.deepEqual([...document.querySelectorAll('.chart-bars li')].map(li => li.getAttribute('aria-label')), ['Nova: 9 requests', 'Atlas: 4 requests']);
    assert.equal(document.querySelectorAll('.chart-table tbody tr').length, 9);
    await change(select('Status'), 'failed');
    const link = document.querySelector('.activity-export');
    assert.match(link.getAttribute('href'), /\/api\/identity\/workspaces\/.+\/activity\/export\?.*status=failed/);
    assert.equal(link.getAttribute('download'), 'cuckoo-activity.csv');
  }, { url, fetch: async path => {
    if (path.includes('/activity/totals')) return response(charted);
    const shared = common(path); if (shared) return shared;
    return response({ items: [], next_before: null });
  } });
});

test('"See what it did" on an agent reply opens its request details', async () => {
  const reads = [];
  await reactFixture(async ({ render, button, click }) => {
    // As in chat-message-actions.test.mjs: jsdom has no IntersectionObserver.
    globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { ChatMessage } = await import('../src/react/chat/message.tsx');
    const message = { id: 'first', chat: 'sales', author: 'nova', name: 'Nova', kind: 'agent', content: 'I sent the quote.', state: 'complete', at: 1 };
    await render(ChatMessage, { message, directory: { users: [], agents: [{ id: 'nova', name: 'Nova' }], role: 'Member' }, replies: 0, openThread() {} });
    await click(button('More actions'));
    await click(button('See what it did'));
    assert.ok(reads.some(path => path.includes('/activity/requests/first')));
    assert.match(document.querySelector('dialog').textContent, /Quote for Sharma Traders/);
  }, { url, fetch: async path => {
    reads.push(path);
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/activity/requests/first')) return response({ ...item('first', 'Quote for Sharma Traders'), message: 'Quote', reply: 'I sent the quote.', steps: [], fix: null, disagreement: null });
    return response({});
  } });
});

test('a link to a scheduled run opens it: its schedule, where the result went, Pause, and no conversation to open', async () => {
  const writes = [];
  const run = '22222222-2222-4222-8222-222222222222';
  const schedule = { id: 'chase', name: 'Monday invoice chase', agent: 'nova', state: 'enabled', revision: 4, principal: 'owner', interval_seconds: 604800,
    post_to: 'finance', posted: 'posted', post_reason: null, room: true };
  await reactFixture(async ({ render, button, click }) => {
    await render(await page(), { href: `${url}&run=${run}` });
    const drawer = document.querySelector('dialog');
    assert.match(drawer.textContent, /Started bySchedule · Monday invoice chase · Every 7 24-hour periods/);
    assert.match(drawer.textContent, /ResultsActivity, and a summary in finance · Posted/);
    assert.equal(drawer.querySelector('a[href*="view=chat"]'), null, 'a work room is not a conversation');
    await click(button('Pause schedule'));
    assert.equal(writes.length, 1);
    assert.equal(writes[0].path, '/api/identity/workspaces/' + w + '/agent-profiles/nova/schedules/chase/state');
    assert.deepEqual({ revision: writes[0].body.revision, enabled: writes[0].body.enabled }, { revision: 4, enabled: false });
  }, { url: `${url}&run=${run}`, fetch: async (path, init = {}) => {
    if (init.method === 'POST') { writes.push({ path, body: JSON.parse(init.body) }); return response({ id: 'chase', state: 'paused', revision: 5 }); }
    const shared = common(path); if (shared) return shared;
    if (path.includes(`/activity/requests/${run}`)) return response({ ...item(run, 'Monday invoice chase', { kind: 'schedule', chat: 'room', conversation_name: 'Monday invoice chase', conversation_kind: 'direct' }),
      message: 'Scheduled work · Monday invoice chase', reply: 'Sent 14 reminders.', steps: [], fix: null, disagreement: null, schedule });
    return response({ items: [], next_before: null });
  } });
});
