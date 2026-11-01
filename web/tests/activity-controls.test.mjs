import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

// Activity's My agents switch and its controls: pause or resume one agent, How to
// fix it, Retry (only the person who asked) and Discuss in chat.
const w = '11111111-1111-4111-8111-111111111111';
const nowUrl = `http://localhost/w/${w}/?view=activity&tab=now`;
const session = { user: { id: 'kavya', email: 'kavya@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Member' }] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const now = Date.now();
const totals = { requests: 0, done: 0, failed: 0, review: 0, waiting: 0, automatic: 0, approved: 0, people: 0, channels: 0, schedule_runs: 0, per_day: [], by_agent: [] };
const card = (id, name, extra = {}) => ({ id, name, role: 'Helper', avatar: null, paused: false, paused_note: null, running: 0, waiting: 0, today: 1,
  failed_today: 0, doing: null, next_run: null, last_failed: null, can_pause: true, ...extra });
const item = (id, summary, extra = {}) => ({ id, chat: 'sales', thread: null, conversation_name: 'sales', conversation_kind: 'channel', in_conversation: true,
  agent: 'atlas', agent_name: 'Atlas', agent_avatar: null, kind: 'chat', requester: 'kavya', requester_name: 'Kavya Nair', summary, state: 'failed',
  latest_update: '', created_ms: now, finished_ms: now + 2000, can_cancel: false, approvals: [] });
const details = (extra = {}) => ({ ...item('failed-1', 'Update the Kapoor order address'), message: 'Update the Kapoor order address',
  reply: 'Zoho Books said the token expired.', steps: [], fix: 'Reconnect Zoho Books, then ask again.', disagreement: null, ...extra });
const common = path => {
  if (path.endsWith('/session')) return response(session);
  if (path.includes('/attention')) return response({ available: true, items: [], next_before: null });
};
async function page() {
  const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
  return (await import('../src/react/activity/activity-page.tsx')).ActivityPage;
}

test('Now opens on My agents, switches to All agents, and remembers the choice on this device', async () => {
  const reads = [];
  await reactFixture(async ({ render, button, click }) => {
    await render(await page(), { href: nowUrl });
    assert.match(reads.filter(path => path.includes('/activity/now')).at(-1), /scope=mine/);
    assert.equal(button('My agents').getAttribute('aria-pressed'), 'true');
    assert.match(document.body.textContent, /My agents/);
    await click(button('All agents'));
    assert.match(reads.filter(path => path.includes('/activity/now')).at(-1), /scope=all/);
    assert.equal(button('All agents').getAttribute('aria-pressed'), 'true');
    assert.equal(localStorage.getItem(`activity.scope.kavya.${w}`), 'all');
  }, { url: nowUrl, fetch: async path => {
    reads.push(path);
    const shared = common(path); if (shared) return shared;
    if (path.includes('/activity/now')) return response({ agents: [card('atlas', 'Atlas')], running: [], away: { since: now, totals } });
  } });
});

test('History asks for the chosen scope and Needs me has no switch', async () => {
  const reads = [];
  await reactFixture(async ({ render, button, click }) => {
    await render(await page(), { href: `http://localhost/w/${w}/?view=activity&tab=history` });
    assert.ok(reads.filter(path => path.includes('/activity/history')).every(path => path.includes('scope=mine')));
    await click(button('Needs me'));
    assert.equal(button('My agents'), undefined);
  }, { url: nowUrl, fetch: async path => {
    reads.push(path);
    const shared = common(path); if (shared) return shared;
    if (path.includes('/activity/totals')) return response(totals);
    if (path.endsWith('/agent-profiles')) return response({ agents: [], role: 'Member' });
    if (path.endsWith('/chat-directory')) return response({ users: [], agents: [] });
    if (path.endsWith('/chats')) return response({ chats: [] });
    return response({ items: [], next_before: null });
  } });
});

test('a manager pauses one agent with one request for a double click; agents they may not pause have no button', async () => {
  const writes = []; let paused = false;
  await reactFixture(async ({ button, render, update }) => {
    await render(await page(), { href: nowUrl });
    assert.equal(button('Pause Iris'), undefined, 'no button without the right to pause');
    await update(() => { const node = button('Pause Atlas'); node.click(); node.click(); });
    assert.equal(writes.length, 1);
    assert.ok(writes[0].path.endsWith('/agent-profiles/atlas/pause'));
    assert.deepEqual(JSON.parse(writes[0].body), { paused: true });
    assert.match(document.body.textContent, /Atlas is paused/);
    assert.ok(button('Resume Atlas'));
  }, { url: nowUrl, fetch: async (path, options = {}) => {
    const shared = common(path); if (shared) return shared;
    if (options.method === 'POST') { writes.push({ path, body: options.body }); paused = true; return response({ paused: true, revision: 2 }); }
    if (path.includes('/activity/now')) return response({ agents: [card('atlas', 'Atlas', { paused }), card('iris', 'Iris', { can_pause: false })], running: [], away: { since: now, totals } });
  } });
});

test('a failing card opens How to fix it, and the person who asked retries once for a double click', async () => {
  const writes = [];
  await reactFixture(async ({ button, click, render, update }) => {
    await render(await page(), { href: nowUrl });
    await click(button('How to fix it'));
    const drawer = document.querySelector('dialog');
    assert.match(drawer.textContent, /Reconnect Zoho Books/);
    await update(() => { const node = button('Retry'); node.click(); node.click(); });
    assert.equal(writes.length, 1);
    assert.ok(writes[0].endsWith('/activity/requests/failed-1/retry'));
    assert.match(document.querySelector('dialog').textContent, /Asked Atlas again/);
  }, { url: nowUrl, fetch: async (path, options = {}) => {
    const shared = common(path); if (shared) return shared;
    if (options.method === 'POST') { writes.push(path); return response({ reply: 'retry-1', chat: 'sales', retry_of: 'failed-1' }); }
    if (path.includes('/activity/requests/failed-1')) return response(details());
    if (path.includes('/activity/now')) return response({ agents: [card('atlas', 'Atlas', { failed_today: 1, last_failed: 'failed-1' })], running: [], away: { since: now, totals } });
  } });
});

test('Retry is not offered to someone else, nor after an unconfirmed action', async () => {
  for (const [label, extra] of [['someone else', { requester: 'ravi', requester_name: 'Ravi Kumar' }],
    ['unconfirmed', { steps: [{ id: 's1', title: 'Refund', effect: 'write', state: 'uncertain', automatic: false, decision: null, decided_by: null, account: 'Razorpay', arguments: {}, at: 1 }] }]]) {
    await reactFixture(async ({ button, render }) => {
      const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
      const { RequestDetails } = await import('../src/react/activity/request-details.tsx');
      await render(RequestDetails, { reply: 'failed-1', close: () => {} });
      assert.match(document.querySelector('dialog').textContent, /Reconnect Zoho Books/, label);
      assert.equal(button('Retry'), undefined, label);
    }, { url: nowUrl, fetch: async path => common(path) ?? response(details(extra)) });
  }
});

test('Discuss in chat opens the direct chat with the agent and starts the message with what it is about', async () => {
  const writes = [];
  await reactFixture(async ({ button, click, navigation, render }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { RequestDetails } = await import('../src/react/activity/request-details.tsx');
    await render(RequestDetails, { reply: 'failed-1', close: () => {} });
    await click(button('Discuss in chat'));
    assert.equal(writes.length, 1, 'one direct chat created');
    assert.deepEqual(JSON.parse(writes[0]).agents, ['atlas']);
    assert.match(navigation.at(-1), new RegExp(`/w/${w}/\\?view=chat&conversation=dm-atlas`));
  }, { url: nowUrl, fetch: async (path, options = {}) => {
    const shared = common(path); if (shared) return shared;
    if (path.endsWith('/chats') && options.method === 'POST') { writes.push(options.body); return response({ id: 'dm-atlas' }); }
    if (path.endsWith('/chats')) return response({ chats: [] });
    return response(details());
  } });
});
