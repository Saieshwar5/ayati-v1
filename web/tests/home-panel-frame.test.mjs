import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

// The panel frame (people-and-agents-first, Phase 1e): who keeps it, what changed,
// honest freshness, what needs you, Show proof and Ask about this; no input.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=home`;
const session = { user: { id: 'kavya', email: 'kavya@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Member' }] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const now = Date.now();
const definition = { title: 'Sales today', purpose: 'Daily sales', template: 'metric', sections: [{ kind: 'metric', label: 'Revenue', value: '₹48,200' }] };
const published = { revision: 1, available: true, definition, saved_ms: now - 20 * 60000, checked_ms: now - 22 * 60000, source_operations: 1,
  chat: 'sales', thread: null, task: 'update-7', generation: 3, what_changed: 'Meesho orders stopped at 9:10; the steel bottle listing is out of stock.' };
const maintenance = (status, extra = {}) => ({ revision: 1, panel_revision: 1, enabled: true, eligible: true, execution_available: true, instructions: 'Update sales',
  interval_seconds: 3600, stale_seconds: 7200, scopes: [], status, checked_ms: published.checked_ms, saved_ms: published.saved_ms, next_ms: null,
  last_update: 'update-7', ...extra });
const panel = (status, extra = {}) => ({ id: '00000000-0000-4000-8000-000000000001', revision: 1, active_revision: 1, agent_name: 'Nova', agent: 'nova',
  active: published, draft: null, maintenance: maintenance(status), ...extra });
const page = panels => ({ available: true, panels, layout: { revision: 0, order: [], wide: [], hidden: [] } });
const waiting = (id, agent, kind = 'approval') => ({ kind, id, version: '1', title: 'Discount for Sharma Traders', status: 'pending', at: now, agent,
  agent_name: agent, chat: 'sales', thread: null, reply: 'r', conversation_name: 'sales', archived: false, read: false, reviewed: false });

async function panels() {
  const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
  return (await import('../src/react/home-panels/panels.tsx')).WorkspacePanels;
}

test('freshness lines come from the server status, never from the agent', async () => {
  const { healthLine, since } = await import('../src/react/home-panels/frame.tsx');
  assert.equal(since(now - 20 * 60000, now), '20 min ago');
  assert.equal(since(now - 3 * 3600000, now), '3 h ago');
  assert.equal(since(now - 26 * 3600000, now), 'yesterday');
  assert.equal(healthLine(panel('current'), published, now), null);
  assert.match(healthLine(panel('delayed'), published, now).text, /^Not updated for 22 min\. These numbers may be out of date\./);
  assert.match(healthLine(panel('delayed'), published, now).fix, /view=activity&tab=history&run=update-7/);
  assert.match(healthLine(panel('failed'), published, now).text, /could not complete\. Showing the previous result/);
  assert.equal(healthLine(panel('blocked'), published, now).fix, undefined);
});

test('a current panel shows who keeps it, what changed, what needs you and its links; never a decision', async () => {
  await reactFixture(async ({ render }) => {
    const WorkspacePanels = await panels();
    await render(WorkspacePanels, { needsYou: { available: true, next_before: null,
      items: [waiting('a1', 'nova'), waiting('q1', 'nova', 'question'), waiting('o1', 'nova', 'outcome'), waiting('a2', 'iris')] } });
    const card = document.querySelector('.home-panel-card');
    assert.match(card.textContent, /What changed: Meesho orders stopped at 9:10/);
    assert.match(card.textContent, /Kept by Nova · for you · Updated 20 min ago · sources read/);
    const needs = card.querySelector('.panel-needs-you');
    assert.match(needs.textContent, /^2 need you/, 'its own approvals and questions, not outcomes or other agents');
    assert.match(needs.href, /view=activity&tab=needs/);
    const proof = [...card.querySelectorAll('a')].find(a => a.textContent === 'Show proof');
    assert.match(proof.href, /view=activity&tab=history&run=update-7/);
    assert.equal(card.querySelector('.panel-stale'), null);
    assert.equal(card.classList.contains('panel-is-stale'), false);
    const labels = [...card.querySelectorAll('button')].map(b => b.textContent);
    assert.ok(labels.includes('Ask about this'));
    assert.ok(!labels.some(label => /^(Approve|Reject|Reply|Answer)/.test(label)), labels.join(', '));
  }, { url, fetch: async path => path.endsWith('/session') ? response(session) : response(page([panel('current')])) });
});

test('a delayed panel says so, dims its numbers and links to the update; no needs-you line when nothing waits', async () => {
  await reactFixture(async ({ render }) => {
    const WorkspacePanels = await panels();
    await render(WorkspacePanels, { needsYou: { available: true, next_before: null, items: [] } });
    const card = document.querySelector('.home-panel-card');
    assert.ok(card.classList.contains('panel-is-stale'));
    const stale = card.querySelector('.panel-stale');
    assert.match(stale.textContent, /Not updated for .*These numbers may be out of date\. How to fix it →/);
    assert.match(stale.querySelector('a').href, /run=update-7/);
    assert.equal(card.querySelector('.panel-needs-you'), null);
  }, { url, fetch: async path => path.endsWith('/session') ? response(session) : response(page([panel('delayed')])) });
});

test('a draft under review gets no frame, and a panel without a note shows none', async () => {
  await reactFixture(async ({ render }) => {
    const WorkspacePanels = await panels();
    await render(WorkspacePanels);
    assert.equal(document.body.textContent.includes('DRAFT_NOTE'), false);
    assert.equal(document.querySelectorAll('.panel-changed').length, 0);
    assert.equal(document.querySelectorAll('.panel-frame-line').length, 1, 'only the published panel');
  }, { url, fetch: async path => path.endsWith('/session') ? response(session)
    : response(page([panel('current', { active: null, active_revision: null, draft: { ...published, what_changed: 'DRAFT_NOTE' }, maintenance: null }),
      panel('current', { id: '00000000-0000-4000-8000-000000000002', active: { ...published, what_changed: null } })])) });
});

test('Ask about this opens the direct chat with the agent once for a double click', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, update, navigation }) => {
    const WorkspacePanels = await panels();
    await render(WorkspacePanels);
    await update(() => { const node = button('Ask about this'); node.click(); node.click(); });
    assert.equal(writes.length, 1);
    assert.deepEqual(JSON.parse(writes[0]).agents, ['nova']);
    assert.match(navigation.at(-1), /view=chat&conversation=dm-nova/);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chats') && options.method === 'POST') { writes.push(options.body); return response({ id: 'dm-nova' }); }
    if (path.endsWith('/chats')) return response({ chats: [] });
    return response(page([panel('current')]));
  } });
});
