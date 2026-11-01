import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

// Supervising at scale (people-and-agents-first, Phase 4): Needs fixing grouped by
// cause with one fix, Coming up with Pause, and similar approvals reviewed together.
const w = '11111111-1111-4111-8111-111111111111';
const nowUrl = `http://localhost/w/${w}/?view=activity&tab=now`;
const needsUrl = `http://localhost/w/${w}/?view=activity&tab=needs`;
const session = { user: { id: 'kavya', email: 'kavya@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Member' }] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const now = Date.now();
const totals = { requests: 0, done: 0, failed: 0, review: 0, waiting: 0, automatic: 0, approved: 0, people: 0, channels: 0, schedule_runs: 0, per_day: [], by_agent: [] };
const card = (id, name, extra = {}) => ({ id, name, role: 'Helper', avatar: null, paused: false, paused_note: null, running: 0, waiting: 0, today: 1,
  failed_today: 0, doing: null, next_run: null, last_failed: null, can_pause: true, ...extra });
async function page() {
  const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
  return (await import('../src/react/activity/activity-page.tsx')).ActivityPage;
}
const fixing = [
  { cause: 'reconnect', title: 'Shopify needs reconnecting · 3 agents stopped', count: 3, fix: { kind: 'settings', label: 'Ask an admin to reconnect', ref: 'c1' },
    agents: [{ id: 'nova', name: 'Nova' }, { id: 'iris', name: 'Iris' }, { id: 'atlas', name: 'Atlas' }], items: [{ agent: 'nova', agent_name: 'Nova', ref: 'c1', detail: 'Shopify', at: now }] },
  { cause: 'reported', title: '1 agent needs a permission', count: 1, fix: { kind: 'need', label: "See what's needed", ref: 'n1' },
    agents: [{ id: 'nova', name: 'Nova' }], items: [{ agent: 'nova', agent_name: 'Nova', ref: 'n1', request: 'r9', cause: 'permission', detail: 'I need refunds allowed', fix: 'Allow order.refund', at: now }] },
];
const coming = [
  { kind: 'routine', id: 's1', agent: 'nova', agent_name: 'Nova', title: 'Morning sales review', at: now + 3600000, revision: 4, can_pause: true },
  { kind: 'follow_up', id: 't1', agent: 'nova', agent_name: 'Nova', title: 'Call Acme back', at: now + 7200000, can_pause: true },
];

test('Now groups what needs fixing by cause with one fix each, and Coming up pauses a routine or a follow-up', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click, update }) => {
    await render(await page(), { href: nowUrl });
    const section = document.querySelector('[aria-label="Needs fixing"]');
    assert.match(section.textContent, /Needs fixing · 2/);
    assert.match(section.textContent, /Shopify needs reconnecting · 3 agents stopped/);
    assert.match([...section.querySelectorAll('a')].find(a => a.textContent === 'Ask an admin to reconnect').href, /view=settings&section=connections/);
    const nova = document.querySelector('article[aria-label="Nova"]');
    assert.match(nova.textContent, /Needs fixing/);
    await click(button("See what's needed"));
    assert.match(document.querySelector('dialog').textContent, /Nova: I need refunds allowed.*Fix: Allow order\.refund/);
    await update(() => { const b = button('Dismiss'); b.click(); });
    assert.ok(writes.some(w => w.path.endsWith('/activity/needs/n1/dismiss')));
    await click(button('Close'));
    const upcoming = document.querySelector('[aria-label="Coming up"]');
    assert.match(upcoming.textContent, /Morning sales review.*Routine · Nova.*Call Acme back.*Follow-up · Nova/);
    await click([...upcoming.querySelectorAll('button')].find(b => b.textContent === 'Pause'));
    const pause = writes.find(w => w.path.includes('/schedules/s1/state'));
    assert.deepEqual({ revision: pause.body.revision, enabled: pause.body.enabled }, { revision: 4, enabled: false });
    await click(button('Cancel follow-up'));
    assert.ok(writes.some(w => w.path.endsWith('/activity/tasks/t1/follow-up/cancel')));
  }, { url: nowUrl, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (options.method === 'POST') { writes.push({ path, body: JSON.parse(options.body ?? '{}') }); return response({ ok: true, id: 's1', state: 'paused', revision: 5 }); }
    if (path.includes('/attention')) return response({ available: true, items: [], next_before: null });
    if (path.includes('/activity/now')) return response({ agents: [card('nova', 'Nova', { needs_fixing: 2 }), card('iris', 'Iris')], running: [],
      away: { since: now, totals }, fixing, coming_up: coming });
    return response({});
  } });
});

const approval = (id, extra = {}) => ({ kind: 'approval', id, version: 'pending', title: `Refund order ${id}`, status: 'pending', at: now, agent: 'nova', agent_name: 'Nova',
  chat: 'sales', thread: null, reply: `r-${id}`, conversation_name: 'sales', archived: false, read: false, reviewed: false,
  operation: 'order.refund', connection: 'shop', operation_title: 'Refund order', created_at: now - 3600000, backup_at: now + 3 * 3600000, ...extra });

test('similar approvals fold into one row, are reviewed together and decided one by one; the most urgent come first', async () => {
  const decisions = [];
  await reactFixture(async ({ render, button, click, update }) => {
    await render(await page(), { href: needsUrl });
    const rows = [...document.querySelectorAll('[aria-label="Approvals"] li')].map(li => li.textContent);
    assert.match(rows[0], /Discount for Sharma.*an admin can decide now/, 'past the backup clock first');
    assert.match(rows[1], /3 × Refund order/);
    await click(button('Review together'));
    const dialog = document.querySelector('dialog');
    assert.match(dialog.textContent, /3 × Refund order/);
    assert.match(dialog.textContent, /Each is decided separately/);
    await update(() => { const b = button('Approve all 3'); b.click(); b.click(); });
    await update(() => {});
    assert.deepEqual(decisions.map(d => d.id).sort(), ['a1', 'a2', 'a3'], 'three separate decisions, once each');
    assert.ok(decisions.every(d => d.body.decision === 'approve' && d.body.id));
  }, { url: needsUrl, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    const decided = path.match(/\/actions\/([^/]+)\/decision$/);
    if (decided) { decisions.push({ id: decided[1], body: JSON.parse(options.body) }); return response({ id: decided[1], state: 'approved', can_decide: false }); }
    const one = path.match(/\/actions\/([^/?]+)$/);
    if (one) return response({ id: one[1], title: `Refund order ${one[1]}`, arguments: { order: one[1], amount: 450 }, can_decide: true, state: 'pending', agent: 'nova', agent_name: 'Nova' });
    if (path.includes('/attention')) return response({ available: true, next_before: null, items: [approval('a1'), approval('a2'), approval('a3'),
      approval('d1', { title: 'Discount for Sharma Traders', operation: 'order.discount', operation_title: 'Discount', created_at: now - 6 * 3600000, backup_at: now - 3600000 })] });
    return response({ agents: [], running: [], away: { since: now, totals } });
  } });
});
