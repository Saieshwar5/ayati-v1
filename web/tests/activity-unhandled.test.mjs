import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

// Events no agent acted on: a line on Now, and their own list in History.
const w = '11111111-1111-4111-8111-111111111111';
const nowUrl = `http://localhost/w/${w}/?view=activity&tab=now`;
const session = { user: { id: 'kavya', email: 'kavya@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Member' }] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const now = Date.now();
const totals = { requests: 0, done: 0, failed: 0, review: 0, waiting: 0, automatic: 0, approved: 0, people: 0, channels: 0, schedule_runs: 0, per_day: [], by_agent: [] };
const event = (id, reason) => ({ id, sequence: 10, at: now, trigger: 'new-orders', title: 'New Shopify order', reason, agent: 'atlas', agent_name: 'Atlas', agent_avatar: null });

test("Now's line opens History on events not handled, each with its reason", async () => {
  const reads = [];
  await reactFixture(async ({ render, button, click }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { ActivityPage } = await import('../src/react/activity/activity-page.tsx');
    await render(ActivityPage, { href: nowUrl });
    assert.match(document.querySelector('.activity-unhandled').textContent, /2 events not handled today/);
    await click(button('See why'));
    const type = [...document.querySelectorAll('label')].find(node => node.textContent.startsWith('Type')).querySelector('select');
    assert.equal(type.value, 'unhandled');
    assert.match(reads.filter(path => path.includes('/activity/unhandled')).at(-1), /scope=mine/);
    assert.equal(reads.some(path => path.includes('/activity/history')), false, 'not asked as requests');
    const list = document.querySelector('[aria-label="Events not handled"]');
    assert.match(list.textContent, /New Shopify order.*All agents were paused/);
    assert.match(list.textContent, /Not handled/);
    assert.equal(document.querySelector('.activity-export'), null, 'no CSV export for events');
  }, { url: nowUrl, fetch: async path => {
    reads.push(path);
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/attention')) return response({ available: true, items: [], next_before: null });
    if (path.includes('/activity/now')) return response({ agents: [], running: [], away: { since: now, totals }, unhandled_today: 2 });
    if (path.includes('/activity/unhandled')) return response({ items: [event('d1', 'All agents were paused when this event arrived, so it was not handled.'), event('d2', null)], next_before: null });
    if (path.includes('/activity/totals')) return response(totals);
    if (path.endsWith('/agent-profiles')) return response({ agents: [], role: 'Member' });
    if (path.endsWith('/chat-directory')) return response({ users: [], agents: [] });
    if (path.endsWith('/chats')) return response({ chats: [] });
    return response({ items: [], next_before: null });
  } });
});

test('Now has no line when every event was handled', async () => {
  await reactFixture(async ({ render }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { ActivityPage } = await import('../src/react/activity/activity-page.tsx');
    await render(ActivityPage, { href: nowUrl });
    assert.equal(document.querySelector('.activity-unhandled'), null);
  }, { url: nowUrl, fetch: async path => {
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/attention')) return response({ available: true, items: [], next_before: null });
    return response({ agents: [], running: [], away: { since: now, totals }, unhandled_today: 0 });
  } });
});
