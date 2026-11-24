// Activity after Design A (designs/activity-next): five tabs, what needs fixing listed in
// Needs you, the conversation under an opened item, keys for an approval, and Work's rows.
import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const at = query => `http://localhost/w/${w}/?view=activity${query}`;
const session = { user: { id: 'meera', email: 'meera@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const now = Date.now();
const totals = { requests: 5, done: 3, failed: 1, review: 2, waiting: 0, automatic: 3, approved: 1, people: 2, channels: 1, schedule_runs: 1, per_day: [], by_agent: [] };
const approval = { kind: 'approval', id: 'a1', version: 'pending', status: 'pending', title: 'Discount for Sharma Traders', at: now, created_at: now - 600000, agent: 'nova', agent_name: 'Nova',
  chat: 'sales', thread: null, reply: 'r-a1', conversation_name: 'sales', archived: false, read: true, reviewed: false };
const fixing = [{ cause: 'reconnect', title: 'Zoho Books needs reconnecting', count: 2, fix: { kind: 'settings', label: 'Reconnect' }, agents: [{ id: 'nova', name: 'Nova' }], items: [{ agent: 'nova', agent_name: 'Nova', ref: 'c1', detail: 'Sign-in expired', at: now }] }];
const running = { id: 'run1', chat: 'sales', thread: null, conversation_name: 'sales', conversation_kind: 'channel', in_conversation: true, agent: 'nova', agent_name: 'Nova', agent_avatar: null, kind: 'chat',
  requester: 'kavya', requester_name: 'Kavya Nair', summary: 'Quote for Sharma Traders', state: 'running', latest_update: '', created_ms: now - 60000, finished_ms: null, can_cancel: true, approvals: [] };
const details = { ...running, id: 'r-a1', state: 'approval', can_cancel: false, message: '@Nova send them a quote with 12% off.', reply: 'The quote is ready. A discount above 10% needs a manager.', steps: [], fix: null, disagreement: null };
const wide = () => { window.matchMedia = () => ({ matches: true, addEventListener() {}, removeEventListener() {} }); };
async function page() {
  const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
  return (await import('../src/react/activity/activity-page.tsx')).ActivityPage;
}
const rowOf = title => [...document.querySelectorAll('.activity-need-row')].find(node => node.textContent.includes(title));
const press = (node, key) => node.dispatchEvent(new window.KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
function server({ member = true } = {}) {
  const state = { reads: [], receipts: [], decisions: [] };
  const fetch = async (path, options = {}) => {
    if (options.method !== 'POST') state.reads.push(path);
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/attention') && options.method === 'POST') { state.receipts.push(JSON.parse(options.body).id); return response({ saved: true }); }
    if (path.includes('/attention')) return response({ available: true, next_before: null, items: state.decisions.length ? [] : [approval] });
    if (path.endsWith('/actions/a1/decision')) { state.decisions.push(JSON.parse(options.body).decision); return response({ id: 'a1', state: 'approved' }); }
    if (path.endsWith('/actions/a1')) return response({ id: 'a1', title: 'Create quote', arguments: { discount: '12%' }, agent: 'nova', agent_name: 'Nova', account_name: 'Zoho Books', conversation_name: 'sales',
      requester_name: 'Kavya Nair', chat: 'sales', thread: null, can_decide: !state.decisions.length, state: state.decisions.length ? 'approved' : 'pending' });
    if (path.includes('/activity/requests/')) return response({ ...details, in_conversation: member });
    if (path.includes('/activity/now')) return response({ agents: [{ id: 'nova', name: 'Nova', role: 'Sales follow-up', avatar: null, paused: false, paused_note: null, running: 1, waiting: 0, today: 9, failed_today: 0,
      doing: 'Quote for Sharma Traders', next_run: null, needs_fixing: 1, can_pause: true }], running: [running], away: { since: now, totals: { ...totals, requests: 0 } }, fixing,
      coming_up: [{ kind: 'follow_up', id: 't1', agent: 'nova', agent_name: 'Nova', title: 'Check if Sharma replied', at: now + 3600000, can_pause: true }] });
    if (path.includes('/activity/totals')) return response(totals);
    if (path.includes('/activity/history')) return response({ items: [{ ...running, id: 'h1', state: 'failed', finished_ms: now }], next_before: null });
    if (path.endsWith('/chats')) return response({ chats: member ? [{ id: 'sales', name: 'sales', kind: 'channel', users: [], agents: [] }] : [] });
    if (path.endsWith('/agent-profiles')) return response({ agents: [{ id: 'nova', name: 'Nova' }], role: 'Admin' });
    if (path.endsWith('/chat-directory')) return response({ users: [], agents: [] });
    if (path.includes('/activity/review')) return response({ available: true, agents: [], people: [], suggestions: [], goals: [], reviews: [], weekly: [] });
    if (path.includes('/activity/upcoming') || path.includes('/activity/tasks')) return response({ items: [], more: false });
    if (path.includes('/agents/pause')) return response({ paused: false, since_ms: null, by: null, note: null, active: 1 });
    return response({});
  };
  return { state, fetch };
}

test('links written for the six earlier tabs open the five that replaced them', async () => {
  const { fetch } = server();
  for (const [old, tab, sign] of [['now', 'Work', /Running now · 1/], ['open', 'Work', /Nothing open/], ['upcoming', 'Calendar', /Nothing runs on its own yet/], ['review', 'Insights', /Last 30 days/], ['needs', 'Needs you', /Discount for Sharma/]]) {
    await reactFixture(async ({ render }) => {
      await render(await page(), { href: at(`&tab=${old}`) });
      assert.equal(document.querySelector('[role=tab][aria-selected=true]').textContent.replace(/\d+$/, ''), tab, `tab=${old}`);
      assert.match(document.body.textContent, sign);
      assert.deepEqual([...document.querySelectorAll('[role=tab]')].map(node => node.textContent.replace(/\d+$/, '')), ['Needs you', 'Work', 'Calendar', 'History', 'Insights']);
    }, { url: at(`&tab=${old}`), fetch });
  }
});

test('what needs fixing opens beside the list with its one fix, and A approves only from inside the list', async () => {
  const { state, fetch } = server();
  await reactFixture(async ({ render, click, update }) => {
    wide();
    await render(await page(), { href: at('&tab=needs') });
    assert.equal(document.querySelector('.activity-count').textContent, '1', 'the red count is still decisions only');
    await click(rowOf('Zoho Books needs reconnecting'));
    assert.deepEqual(state.receipts, [], 'a cause to fix has no read receipt');
    const pane = document.querySelector('.ui-pane');
    assert.match(pane.textContent, /Needs fixing.*Nova: Sign-in expired/s);
    assert.match([...pane.querySelectorAll('a')].find(a => a.textContent === 'Reconnect').getAttribute('href'), /view=settings&section=connections/);

    await click(rowOf('Discount for Sharma Traders'));
    await update(() => press(document.body, 'a'));
    assert.deepEqual(state.decisions, [], 'a stray key outside the list decides nothing');
    await update(() => { rowOf('Discount for Sharma Traders').focus(); press(rowOf('Discount for Sharma Traders'), 'a'); });
    assert.deepEqual(state.decisions, ['approve']);
  }, { url: at('&tab=needs'), fetch });
});

test('the conversation shows under an approval for someone in it, with the way to Chat and back', async () => {
  const { state, fetch } = server();
  await reactFixture(async ({ render, click }) => {
    wide();
    await render(await page(), { href: at('&tab=needs') });
    await click(rowOf('Discount for Sharma Traders'));
    const talk = document.querySelector('[aria-label="Conversation"]');
    assert.match(talk.textContent, /Kavya Nair.*@Nova send them a quote with 12% off\..*Nova.*The quote is ready/s);
    const link = [...talk.querySelectorAll('a')].find(a => a.textContent === 'Open in Chat ›').getAttribute('href');
    assert.match(link, /view=chat&conversation=sales&message=r-a1&from=activity&item=approval%2Fa1%2Fpending$/);
    assert.match(window.location.search, /item=approval%2Fa1%2Fpending/, 'the open row is kept in the address');
    assert.equal(talk.querySelector('textarea'), null, 'only the person who asked can add to a running request');
  }, { url: at('&tab=needs'), fetch });
});

test('someone outside the conversation is asked before it is read, and a link to a read row reopens it without a new receipt', async () => {
  const { state, fetch } = server({ member: false });
  await reactFixture(async ({ render, click, button }) => {
    wide();
    await render(await page(), { href: at('&tab=needs&item=approval%2Fa1%2Fpending') });
    assert.deepEqual(state.receipts, [], 'coming back from Chat saves no receipt');
    assert.ok(document.querySelector('.ui-pane'), 'the row it left is open again');
    const talk = document.querySelector('[aria-label="Conversation"]');
    assert.match(talk.textContent, /You are not in sales.*recorded in the audit log/);
    assert.equal(state.reads.some(path => path.includes('/activity/requests/')), false, 'nothing is read, so nothing is recorded');
    await click(button('Read the request'));
    assert.ok(state.reads.some(path => path.includes('/activity/requests/r-a1')));
    assert.match(document.querySelector('[aria-label="Conversation"]').textContent, /nothing to open in Chat/);
  }, { url: at('&tab=needs&item=approval%2Fa1%2Fpending'), fetch });
});

test('Work lists agents as rows; a name opens what waits on you, what it is doing and what comes next, and leads to Needs you', async () => {
  const { fetch } = server();
  await reactFixture(async ({ render, click, button }) => {
    wide();
    await render(await page(), { href: at('&tab=work') });
    const row = document.querySelector('article[aria-label="Nova"]');
    assert.match(row.textContent, /Sales follow-up.*Needs fixing.*1 thing needs fixing · in Needs you/);
    assert.ok(button('Pause Nova'), 'the row keeps its own controls');
    await click(button('Nova, details'));
    const pane = document.querySelector('.ui-pane');
    assert.match(pane.textContent, /Needs you · 2.*Discount for Sharma Traders.*Zoho Books needs reconnecting.*Doing now · 1.*Quote for Sharma Traders.*Coming up.*Check if Sharma replied.*9 requests/s);
    assert.equal(row.getAttribute('aria-current'), 'true');
    await click([...pane.querySelectorAll('button')].filter(node => node.textContent === 'Open in Needs you')[1]);
    assert.equal(button('Needs you1').getAttribute('aria-selected'), 'true');
    assert.equal(document.querySelector('.ui-pane h2').textContent, 'Zoho Books needs reconnecting', 'that row is open');
  }, { url: at('&tab=work'), fetch });
});

test('History opens on the log; "need a look" filters it, a row opens beside it, and the pause switch sits in a menu on Work', async () => {
  const { state, fetch } = server();
  await reactFixture(async ({ render, click, button }) => {
    wide();
    await render(await page(), { href: at('&tab=history') });
    assert.match(document.querySelector('[aria-label="Totals"]').textContent, /5 requests · 3 done · 2 need a look/);
    await click(button('2 need a look'));
    assert.match(state.reads.filter(path => path.includes('/activity/history')).at(-1), /status=review/);
    await click(document.querySelector('button.activity-row-button'));
    assert.equal(document.querySelector('dialog'), null);
    assert.match(document.querySelector('.ui-pane').textContent, /Quote for Sharma Traders/);
    await click(button('Work'));
    assert.ok(document.querySelector('details.activity-more').contains(button('Pause all agents')));
  }, { url: at('&tab=history'), fetch });
});
