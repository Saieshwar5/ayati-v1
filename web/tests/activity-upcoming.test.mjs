// Activity › Upcoming (designs/automations-simplified 3): everything agents will do on
// their own, grouped by day, with Pause / Resume in place and links to where it's set up.
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
const schedule = (id, title, extra = {}) => ({ kind: 'schedule', id, agent: 'atlas', agent_name: 'Atlas', title, at: now + 3600000, state: 'enabled', revision: 2,
  interval_seconds: 86400, timezone: 'Asia/Kolkata', detail: '', by: 'Ravi Kumar', own: true, ...extra });
const items = [
  schedule('s1', 'Morning stock check'),
  schedule('s2', 'Month-end report', { state: 'paused', at: null, by: 'Meera Iyer', own: false }),
  schedule('s3', 'Bank reconciliation', { agent: 'ledger', agent_name: 'Ledger', by: 'Meera Iyer', own: false, at: now + 7200000 }),
  { kind: 'follow_up', id: 't1', agent: 'nova', agent_name: 'Nova', title: 'Check if Sharma replied', at: now + 5400000 },
  { kind: 'trigger', id: 'e1', agent: 'atlas', agent_name: 'Atlas', title: 'New order in Zoho Books', source: 'Zoho Books · order created', state: 'enabled', revision: 3, detail: '', by: 'Ravi Kumar', own: true },
  { kind: 'app_trigger', agent: 'atlas', agent_name: 'Atlas', id: 'returns', app: 'returns', app_name: 'Returns', change: 'added', collection: 'returns', title: 'New returns in Returns', by: 'Kavya', manages: false },
];

async function open(render, update) {
  const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
  const { ActivityPage } = await import('../src/react/activity/activity-page.tsx');
  await render(ActivityPage, { href: url });
  await update(async () => { await new Promise(resolve => setTimeout(resolve, 20)); });
}
const row = title => [...document.querySelectorAll('.activity-upcoming li')].find(li => li.textContent.includes(title));
const within = (node, label) => [...node.querySelectorAll('button,a')].find(b => b.textContent === label);

test('Upcoming groups what runs by day, then paused, then always on, and filters by kind', async () => {
  const reads = [];
  await reactFixture(async ({ render, update, click, button }) => {
    await open(render, update);
    assert.equal(document.querySelector('[role=tab][aria-selected=true]').textContent, 'Upcoming');
    assert.ok(reads.some(path => path.endsWith('/activity/upcoming?scope=mine')));
    const groups = [...document.querySelectorAll('.activity-upcoming h2')].map(h => h.textContent);
    assert.deepEqual(groups.slice(-2), ['Paused', 'Always on']);
    const order = [...document.querySelectorAll('.activity-upcoming li strong')].map(s => s.textContent);
    assert.deepEqual(order, ['Morning stock check', 'Check if Sharma replied', 'Bank reconciliation', 'Month-end report', 'New order in Zoho Books', 'New returns in Returns']);
    assert.match(row('Morning stock check').textContent, /Schedule · Atlas · Every day · set up by you/);
    assert.match(row('New returns in Returns').textContent, /Trigger · Atlas · Added from Returns.*Open in Apps/);
    assert.equal(within(row('Morning stock check'), 'Edit in Atlas').getAttribute('href'), `/w/${w}/?view=agents&selected=atlas&section=automations`);
    await click(button('Triggers'));
    assert.deepEqual([...document.querySelectorAll('.activity-upcoming li strong')].map(s => s.textContent), ['New order in Zoho Books', 'New returns in Returns']);
  }, { url, fetch: async path => {
    reads.push(path);
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/attention')) return response({ available: true, items: [], next_before: null });
    if (path.includes('/activity/upcoming')) return response({ items });
    return response({});
  } });
});

test('Pause and Undo happen in place; only its person resumes; triggers resume on the agent; follow-ups cancel', async () => {
  const writes = [];
  await reactFixture(async ({ render, update, click }) => {
    await open(render, update);
    await update(() => { const b = within(row('Morning stock check'), 'Pause'); b.click(); b.click(); });
    await update(async () => { await new Promise(resolve => setTimeout(resolve, 10)); });
    const pauses = writes.filter(x => x.path.endsWith('/agent-profiles/atlas/schedules/s1/state'));
    assert.equal(pauses.length, 1, 'a double click pauses once');
    assert.deepEqual({ revision: pauses[0].body.revision, enabled: pauses[0].body.enabled }, { revision: 2, enabled: false });
    assert.match(document.querySelector('.activity-upcoming [role=status]').textContent, /Morning stock check is paused\. Undo/);
    await click(document.querySelector('.activity-undo'));
    const undo = writes.at(-1);
    assert.deepEqual({ revision: undo.body.revision, enabled: undo.body.enabled }, { revision: 3, enabled: true });
    assert.notEqual(undo.body.request, pauses[0].body.request);

    await click(within(row('Bank reconciliation'), 'Pause'));
    assert.match(document.querySelector('.activity-upcoming [role=status]').textContent, /Only Meera Iyer can resume it\./);
    assert.equal(document.querySelector('.activity-undo'), null, 'no Undo when someone else must resume');
    assert.match(row('Month-end report').textContent, /Only Meera Iyer can resume it\./);
    assert.equal(within(row('Month-end report'), 'Resume'), undefined);

    await click(within(row('New order in Zoho Books'), 'Pause'));
    const trigger = writes.at(-1);
    assert.ok(trigger.path.endsWith('/agent-profiles/atlas/event-triggers/e1/state'));
    assert.deepEqual({ revision: trigger.body.revision, enabled: trigger.body.enabled, connection_revision: trigger.body.connection_revision }, { revision: 3, enabled: false, connection_revision: null });

    await click(within(row('Check if Sharma replied'), 'Cancel follow-up'));
    assert.ok(writes.at(-1).path.endsWith('/activity/tasks/t1/follow-up/cancel'));
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (options.method === 'POST') { writes.push({ path, body: JSON.parse(options.body ?? '{}') }); return response({ id: 's1', state: 'paused', revision: 3 }); }
    if (path.includes('/attention')) return response({ available: true, items: [], next_before: null });
    if (path.includes('/activity/upcoming')) return response({ items });
    return response({});
  } });
});
