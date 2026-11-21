import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

// One agent's page (profile/profile-page.tsx, 2026-11-21): six rows of the real
// things, ＋ to add to a row, and a right-hand column that is Recent work or the
// editor for what was clicked.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const session = { user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Member' }] };
const settings = { provider: '', instructions: 'Be brief.\nAsk Ravi before refunds.', escalation: '', avatar: 'robot-03', memory: true, browser: false, history: 'all', paused: false };
const agent = { id: 'atlas', name: 'Atlas', role: 'Orders assistant', model: 'fixture', audience: 'everyone', revision: 5, settings, editable: true, owner_id: 'owner', owner_name: 'Ravi Kumar', people: [] };
const directory = { role: 'Member', agents: [agent], default_model: 'fixture', providers: [], system_prompt: 'System guide', web_search: true };
const operation = { id: 'order.read', title: 'Read orders', effect: 'read', meaning: 'read', workspace_mode: 'allow', needs_other: false, agent_mode: null, mode: 'allow', revision: 0 };
const shop = { id: 'shop', server: 'shopify', name: 'Shopify', account: 'lotus-store', revision: 1, on: true, operations: [operation], own: true };
const mail = { id: 'mail', server: 'gmail', name: 'Gmail', revision: 1, on: true, operations: [], own: true, state: 'reconnect' };
const routine = { id: 'r1', name: 'Late orders', content: 'List late orders', chat: null, first_ms: 1, interval_seconds: 86400, timezone: 'UTC', principal: 'owner', principal_name: 'Ravi Kumar', chat_name: null, revision: 1, state: 'enabled', next_ms: null, detail: '', can_resume: true };
const request = { id: 'reply', sequence: 1, chat: 'channel', thread: null, source_message: 'm', conversation_name: 'Orders', requester: 'owner', requester_name: 'Ravi Kumar', summary: 'Where is order 4417?', state: 'complete', latest_update: '', created_ms: 1000, can_cancel: false, cancellation: null, approvals: [] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });

function server({ reads = [], editable = true, apps = [shop, mail] } = {}) {
  return async path => {
    const address = new URL(path, 'http://localhost'), route = address.pathname.replace(`/api/identity/workspaces/${w}`, '');
    reads.push(route + address.search);
    if (route.endsWith('/session')) return response(session);
    if (route.endsWith('/atlas/skills')) return response({ skills: [{ id: 's1', name: 'refund-check', shared: false, version: 2 }], editable });
    if (route.endsWith('/atlas/knowledge')) return response({ notes: [{ id: 'n1', title: 'Delivery', content: 'Two days.', forgotten: false }, { id: 'n2', title: 'Old', content: '', forgotten: true }] });
    if (route.includes('/library-always/')) return response({ items: [{ id: 'd1', title: 'Returns policy', use_for: '', characters: 10 }], given: [], hidden: 0, chosen: 1, editable, most: 5, budget: 8000 });
    if (route.endsWith('/atlas/learning')) return response({ available: true, configured: true, enabled: true, revision: 1, proposals: [{ id: 'l1', state: 'proposed', title: 'Tracking link', content: '', why: '', kind: 'new', created_ms: 1, evidence: [], conversation: null, replaces: null }] });
    if (route.endsWith('/atlas/control')) return response({ can_manage: editable, can_manage_connections: editable, viewer: 'owner', connections: apps, actions: [], execution_available: true });
    if (route.endsWith('/workspace-computers')) return response({ computers: [], can_change: editable });
    if (route.endsWith('/atlas/schedules')) return response({ schedules: [routine], conversations: [], more_conversations: false, execution_available: true });
    if (route.includes('/requests')) return response({ requests: [request], next_before: null });
    if (route.includes('/atlas/systems')) return response({ systems: [], can_connect: editable });
    return response({});
  };
}
async function open(render, props = {}) {
  await (await import('../src/workspace-context.ts')).enterWorkspace();
  const { AgentWorkspace } = await import('../src/react/agents/workspace.tsx');
  await render(AgentWorkspace, { agent, directory, back() {}, openChat() {}, busy: false, saved: async () => {}, ...props });
}
const row = name => document.querySelector(`.profile-row[aria-label="${name}"]`);
const chips = name => [...row(name).querySelectorAll('.profile-chip')].map(node => node.textContent);
const panel = () => document.querySelector('.profile-panel');

test('each row shows the real things: skills, what it knows, apps with their state, schedules and people', async () => {
  await reactFixture(async ({ render }) => {
    await open(render);
    assert.equal(row('Does').querySelector('.profile-rules').textContent, settings.instructions);
    assert.deepEqual(chips('Does').slice(0, 1), ['refund-check']);
    assert.deepEqual(chips('Knows').slice(0, 3), ['Notes 1', 'Documents 1', 'Lessons1 to review'], 'a forgotten note is not counted');
    assert.deepEqual(chips('Can use').slice(0, 3), ['SShopify', 'GGmailNeeds reconnecting', 'Web search']);
    assert.ok(row('Can use').querySelector('.profile-chip .profile-dot.warn'), 'an app that needs signing in again has an amber dot');
    assert.match(row('On its own').querySelector('.profile-line').textContent, /Late orders · Every day/);
    assert.deepEqual(chips('People'), ['RKYou owner', 'Everyone can message it']);
    assert.equal(document.querySelector('.profile-work li a').getAttribute('href'), `/w/${w}/?view=chat&conversation=channel&message=reply`);
  }, { url, fetch: server() });
});

test('＋ asks which kind when a row holds several; a thing opens its editor in place of Recent work, and Close brings it back', async () => {
  const reads = [];
  await reactFixture(async ({ render, button, click, update }) => {
    await open(render);
    assert.equal(document.querySelector('[role=menu]'), null);
    await click(button('Add an app, computer or MCP server'));
    assert.deepEqual([...document.querySelectorAll('[role=menuitem]')].map(node => node.textContent), ['App', 'Computer', 'MCP server']);
    assert.equal(document.activeElement.textContent, 'App', 'the menu takes the keyboard');
    await update(() => document.activeElement.dispatchEvent(new window.KeyboardEvent('keydown', { key: 'Escape', bubbles: true })));
    assert.equal(document.querySelector('[role=menu]'), null);
    assert.equal(document.activeElement, button('Add an app, computer or MCP server'), 'Escape returns to the ＋');
    // One app, alone, with its rule per action.
    await click([...row('Can use').querySelectorAll('button.profile-chip')].find(node => node.textContent.includes('Shopify')));
    assert.equal(panel().getAttribute('aria-label'), 'Can use');
    assert.equal(document.querySelector('.profile-work'), null);
    assert.deepEqual([...panel().querySelectorAll('article.agent-tool')].map(node => node.getAttribute('aria-label')), ['Shopify'], 'only the app that was clicked');
    assert.match(panel().textContent, /Read orders/);
    assert.equal(panel().querySelector('.agent-tool-actions'), null, 'no recent actions here: work is decided elsewhere');
    assert.equal([...row('Can use').querySelectorAll('[aria-current=true]')].length, 1, 'the open chip is marked');
    const before = reads.filter(route => route.endsWith('/atlas/skills')).length;
    await click(button('Close'));
    assert.ok(document.querySelector('.profile-work'), 'Recent work is back');
    assert.equal(reads.filter(route => route.endsWith('/atlas/skills')).length, before + 1, 'closing reads the rows again');
    await click(button('Lessons1 to review'));
    assert.equal(panel().getAttribute('aria-label'), 'Lessons from its work');
  }, { url, fetch: server({ reads }) });
});

test('a link to a section opens the panel that holds it', async () => {
  await reactFixture(async ({ render }) => {
    const { agentSectionUrl, panelFor, sectionNamed } = await import('../src/react/agents/workspace-tabs.tsx');
    assert.equal(agentSectionUrl('atlas', 'Connections', 'shop/order.read'), `/w/${w}/?view=agents&selected=atlas&section=connections&action=shop%2Forder.read`);
    assert.deepEqual(panelFor(sectionNamed('tools'), 'shop/order.read'), { kind: 'app', id: 'shop', focus: 'shop/order.read' }, 'older names still work');
    assert.deepEqual(panelFor(sectionNamed('connections')), { kind: 'connections' });
    assert.deepEqual(panelFor(sectionNamed('environment')), { kind: 'model' });
    assert.equal(panelFor(sectionNamed('instructions')), undefined, 'the rules are on the page itself');
    await open(render, { start: panelFor(sectionNamed('connections'), 'shop/order.read') });
    assert.ok(panel().querySelector('[data-action="shop/order.read"]'), 'the linked rule is in the panel');
  }, { url, fetch: server() });
});

test('someone who cannot change the agent reads the same page: no ＋, no pencil, chips that only show', async () => {
  const reads = [];
  await reactFixture(async ({ render, button }) => {
    await open(render, { agent: { ...agent, editable: false, owner_id: 'priya', owner_name: 'Priya Shah' } });
    assert.equal(document.querySelector('.profile-add'), null);
    assert.equal(button('Edit the rules'), undefined);
    assert.equal(row('Does').querySelector('p.profile-rules').textContent, settings.instructions);
    assert.equal(row('People').querySelector('button'), null);
    assert.equal(row('Model').querySelector('button'), null);
    assert.deepEqual(chips('People'), ['PSPriya Shah owner', 'Everyone can message it']);
    assert.equal(chips('Knows').some(text => text.startsWith('Lessons')), false, 'lessons are for those who look after it');
    assert.equal(document.querySelector('.profile-work h3').textContent, 'Your requests');
    assert.equal(reads.some(route => /\/(learning|schedules)(\?|$)/.test(route)), false, 'what only its owners manage is not read');
    assert.ok(reads.some(route => route.includes('/requests?scope=mine')), 'only their own requests are asked for');
  }, { url, fetch: server({ reads, editable: false }) });
});
