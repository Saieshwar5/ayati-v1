import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
// The Instructions tab also lists the documents an agent always reads (Library LB2c).
const noAlways = { items: [], given: [], hidden: 0, chosen: 0, editable: false, most: 5, budget: 8000 };
const workspace = '11111111-1111-4111-8111-111111111111';
const session = { user: { id: 'owner', email: 'owner@example.test' }, simulation: true, workspaces: [{ id: workspace, name: 'Store', role: 'Admin' }] };
const settings = { provider: '', instructions: '', escalation: 'When delivery details are unclear', avatar: 'spark', memory: false, browser: false, history: 'all', paused: false };
const agent = { id: 'alpha', name: 'Meera', role: 'Coordinate supplier work', model: 'fixture', audience: 'selected', revision: 1, settings, editable: true, owner_name: 'Ananya' };
const directory = { role: 'Admin', agents: [agent, { ...agent, id: 'beta', name: 'Tara', role: 'Sales coordination' }], default_model: 'fixture', providers: [], system_prompt: 'System guide' };
const controls = { can_manage: true, connections: [], rules: [], actions: [], servers: [], conversations: [], delegations: [], viewer: 'owner' };
const request = { id: 'reply', sequence: 1, chat: 'channel', thread: 'root', source_message: 'original', conversation_name: 'Procurement', requester: 'owner', requester_name: 'Ananya', summary: 'Check supplier delivery', state: 'approval', latest_update: 'Comparison saved; waiting for approval.', created_ms: 1000, can_cancel: true, approvals: [] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const empty = { requests: [], next_before: null };

function read(path) {
  if (path.endsWith('/session')) return session;
  if (path.includes('/requests')) return empty;
  if (path.endsWith('/control')) return controls;
  if (path.endsWith('/knowledge')) return { notes: [] };
  if (path.endsWith('/chats')) return { chats: [] };
  return directory;
}

test('selected team workspace keeps selection while searching and does not create conversations on mount', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click, change, update }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { AgentsPage } = await import('../src/react/agents/directory.tsx');
    await render(AgentsPage);
    assert.equal(document.querySelector('.agent-identity-title h2').textContent, 'Meera');
    await click(button('Open agent Tara'));
    assert.equal(document.querySelector('.agent-identity-title h2').textContent, 'Tara');
    assert.equal(document.querySelector('[aria-label="Find an agent"]'), null, 'two agents need no search box');
    assert.equal(document.querySelector('.agent-roster footer'), null);
    assert.equal(writes.length, 0);
    // One page per agent: rows, no tabs, and nothing written by looking.
    assert.equal(document.querySelector('[role=tablist]'), null);
    assert.equal(document.querySelectorAll('.profile-row').length, 6);
    assert.equal(document.body.textContent.includes('Not connected'), false);
    assert.equal(writes.length, 0);
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async (path, options = {}) => {
    if (path.includes('/library-always/')) return { ok: true, status: 200, json: async () => noAlways };
    if (options.method === 'POST') writes.push(path);
    return response(read(path));
  } });
});

test('the list groups your assistant, the team and members’ assistants, shows each state, and searches only when long', async () => {
  const mine = { ...agent, id: 'mine', name: 'Ayati', role: 'Your assistant', assistant: true, is_owner: true };
  const theirs = { ...agent, id: 'theirs', name: 'Ayati', role: 'Your assistant', assistant: true, is_owner: false, owner_name: 'Dev Singh', can_talk: false };
  const team = [agent, { ...agent, id: 'beta', name: 'Tara', role: 'Sales coordination', pending: 2 }, { ...agent, id: 'gamma', name: 'Nova', role: 'Follow-up', settings: { ...settings, paused: true } }];
  const more = Array.from({ length: 5 }, (_, n) => ({ ...agent, id: `extra-${n}`, name: `Helper ${n}`, role: 'Helps' }));
  let list = [theirs, ...team, mine];
  await reactFixture(async ({ render, button, change }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { AgentsPage } = await import('../src/react/agents/directory.tsx');
    await render(AgentsPage);
    const roster = document.querySelector('.agent-roster-scroll');
    assert.deepEqual([...roster.querySelectorAll('h2')].map(node => node.textContent), ['Yours', 'Team 3', 'Members’ assistants 1']);
    assert.deepEqual([...roster.querySelectorAll('.agent-team-row')].map(node => node.textContent),
      ['AyatiYour assistant', 'MeeraCoordinate supplier work', 'TaraSales coordination2 working', 'NovaFollow-upPaused', 'AyatiDev Singh’s assistant']);
    // Two assistants share a name: each row's name says whose it is, and its state.
    assert.ok(button('Open agent Ayati, Dev Singh’s assistant'));
    assert.ok(button('Open agent Tara, 2 working').querySelector('.agent-dot-working'));
    assert.ok(button('Open agent Nova, Paused').querySelector('.agent-dot-paused'));
    assert.ok(button('Open agent Meera').querySelector('.agent-dot-ready'));
    assert.equal(document.querySelector('[aria-label="Find an agent"]'), null, 'five agents need no search box');
    list = [...list, ...more];
    const { AgentRoster } = await import('../src/react/agents/agent-roster.tsx');
    await render(AgentRoster, { agents: list, loaded: true, retry() {}, current: 'beta', locked: false, begin() {}, select() {} });
    await change(document.querySelector('[aria-label="Find an agent"]'), 'dev singh');
    assert.deepEqual([...document.querySelectorAll('.agent-team-row')].map(node => node.textContent), ['AyatiDev Singh’s assistant'], 'ten agents: search, also by whose assistant it is');
    await change(document.querySelector('[aria-label="Find an agent"]'), 'nobody');
    assert.match(document.querySelector('.ui-empty').textContent, /No matching agents/);
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async path => {
    if (path.includes('/library-always/')) return { ok: true, status: 200, json: async () => noAlways };
    return response(path.endsWith('/agent-profiles') ? { ...directory, agents: list } : read(path));
  } });
});

test('Recent work leads to the conversation, or to Activity while a person is waited for; nothing is approved or stopped here', async () => {
  const waiting = { ...request, approvals: [{ id: 'approval', title: 'Send supplier email', state: 'pending' }] };
  const done = { ...request, id: 'done', thread: null, summary: 'Compare quotes', state: 'complete' };
  await reactFixture(async ({ render }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { RecentWork } = await import('../src/react/agents/profile/recent-work.tsx');
    await render(RecentWork, { requests: [waiting, done], all: true, loaded: true });
    const links = [...document.querySelectorAll('.profile-work li a')];
    assert.deepEqual(links.map(link => link.getAttribute('href')), [`/w/${workspace}/?view=activity&tab=needs`, `/w/${workspace}/?view=chat&conversation=channel&message=done`]);
    assert.equal(links[0].getAttribute('aria-label'), 'Check supplier delivery, Waiting for approval, opens in Activity');
    assert.match(links[0].querySelector('.profile-pill').textContent, /Waiting for approval/);
    assert.equal(links[1].querySelector('.profile-pill'), null);
    assert.equal(document.querySelector('.profile-work button'), null, 'the agent page decides nothing');
    await render(RecentWork, { requests: [], all: false, loaded: true });
    assert.equal(document.querySelector('.profile-work h3').textContent, 'Your requests');
    assert.match(document.querySelector('.profile-work').textContent, /Nothing yet/);
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async path => response(path.endsWith('/session') ? session : {}) });
});

test('switching selected agent during Message cannot create or navigate a stale conversation', async () => {
  let chats; const writes = [];
  await reactFixture(async ({ render, button, click, update, navigation }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { AgentsPage } = await import('../src/react/agents/directory.tsx');
    await render(AgentsPage); await click(button('Message'));
    await click(button('Open agent Tara'));
    await update(() => chats(response({ chats: [] })));
    assert.equal(writes.length, 0); assert.equal(navigation.length, 0);
    assert.equal(document.querySelector('.agent-identity-title h2').textContent, 'Tara');
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async (path, options = {}) => {
    if (path.includes('/library-always/')) return { ok: true, status: 200, json: async () => noAlways };
    if (options.method === 'POST') writes.push(path);
    if (path.endsWith('/chats')) return new Promise(resolve => { chats = resolve; });
    return response(read(path));
  } });
});
