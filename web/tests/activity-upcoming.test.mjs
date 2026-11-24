// Activity › Calendar › List (designs/automations-simplified 3): what agents will do on
// their own, grouped by day, with Pause in place and links to where it's set up.
// Schedules became tasks in schema 147, so none are listed as schedules.
import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=activity&tab=upcoming`;
const session = { user: { id: 'owner', email: 'owner@example.test', display_name: 'Ravi Kumar' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };
const response = data => ({ ok: true, status: 200, json: async () => data });
const now = Date.now();
const items = [
  { kind: 'follow_up', id: 't2', agent: 'atlas', agent_name: 'Atlas', title: 'Morning stock check', at: now + 3600000 },
  { kind: 'follow_up', id: 't1', agent: 'nova', agent_name: 'Nova', title: 'Check if Sharma replied', at: now + 5400000 },
  { kind: 'due', id: 't3', agent: 'ledger', agent_name: 'Ledger', title: 'Bank reconciliation', at: now + 7200000 },
  { kind: 'trigger', id: 'e1', agent: 'atlas', agent_name: 'Atlas', title: 'New order in Zoho Books', source: 'Zoho Books · order created', state: 'enabled', revision: 3, detail: '', by: 'Ravi Kumar', own: true },
  { kind: 'trigger', id: 'e2', agent: 'atlas', agent_name: 'Atlas', title: 'New email', source: 'Gmail', state: 'paused', revision: 1, detail: '', by: 'Meera Iyer', own: false },
];

async function open(render, update) {
  const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
  const { ActivityPage } = await import('../src/react/activity/activity-page.tsx');
  await render(ActivityPage, { href: url });
  await update(async () => { await new Promise(resolve => setTimeout(resolve, 20)); });
}
const row = title => [...document.querySelectorAll('.activity-upcoming li')].find(li => li.textContent.includes(title));
const within = (node, label) => [...node.querySelectorAll('button,a')].find(b => b.textContent === label);

test('Planned groups what runs by day, then always on, and filters by kind', async () => {
  const reads = [];
  await reactFixture(async ({ render, update, click, button }) => {
    await open(render, update);
    assert.equal(document.querySelector('[role=tab][aria-selected=true]').textContent, 'Calendar', 'Planned is now the Calendar; old links open its list');
    assert.ok(reads.some(path => path.endsWith('/activity/upcoming?scope=mine')));
    const groups = [...document.querySelectorAll('.activity-upcoming h2')].map(h => h.textContent);
    assert.deepEqual(groups.slice(-1), ['Always on']);
    const order = [...document.querySelectorAll('.activity-upcoming li strong')].map(s => s.textContent);
    assert.deepEqual(order, ['Morning stock check', 'Check if Sharma replied', 'Bank reconciliation', 'New order in Zoho Books', 'New email']);
    assert.equal(button('Schedules'), undefined, 'no schedule filter any more');
    assert.equal(within(row('Morning stock check'), 'Open task').getAttribute('href'), `/w/${w}/?view=activity&tab=open&task=t2`);
    assert.equal(within(row('New order in Zoho Books'), 'Edit in Atlas').getAttribute('href'), `/w/${w}/?view=agents&selected=atlas&section=automations`);
    await click(button('Triggers'));
    assert.deepEqual([...document.querySelectorAll('.activity-upcoming li strong')].map(s => s.textContent), ['New order in Zoho Books', 'New email']);
  }, { url, fetch: async path => {
    reads.push(path);
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/attention')) return response({ available: true, items: [], next_before: null });
    if (path.includes('/activity/upcoming')) return response({ items });
    return response({});
  } });
});

test('Pause happens in place once; triggers resume on the agent; follow-ups cancel', async () => {
  const writes = [];
  await reactFixture(async ({ render, update, click }) => {
    await open(render, update);
    await update(() => { const b = within(row('New order in Zoho Books'), 'Pause'); b.click(); b.click(); });
    await update(async () => { await new Promise(resolve => setTimeout(resolve, 10)); });
    const pauses = writes.filter(x => x.path.endsWith('/agent-profiles/atlas/event-triggers/e1/state'));
    assert.equal(pauses.length, 1, 'a double click pauses once');
    assert.deepEqual({ revision: pauses[0].body.revision, enabled: pauses[0].body.enabled, connection_revision: pauses[0].body.connection_revision }, { revision: 3, enabled: false, connection_revision: null });
    assert.match(document.querySelector('.activity-upcoming [role=status]').textContent, /New order in Zoho Books is paused\. Resume it on Atlas’s Automations tab/);
    assert.equal(within(row('New email'), 'Review and resume in Atlas').getAttribute('href'), `/w/${w}/?view=agents&selected=atlas&section=automations`);

    await click(within(row('Check if Sharma replied'), 'Cancel follow-up'));
    assert.ok(writes.at(-1).path.endsWith('/activity/tasks/t1/follow-up/cancel'));
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (options.method === 'POST') { writes.push({ path, body: JSON.parse(options.body ?? '{}') }); return response({ revision: 4 }); }
    if (path.includes('/attention')) return response({ available: true, items: [], next_before: null });
    if (path.includes('/activity/upcoming')) return response({ items });
    return response({});
  } });
});
