import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
const workspace = '11111111-1111-4111-8111-111111111111';
const session = { user: { id: 'owner', email: 'owner@example.test' }, simulation: true, workspaces: [{ id: workspace, name: 'Store', role: 'Owner' }] };
const settings = { provider: '', instructions: '', escalation: 'When delivery details are unclear', avatar: 'spark', memory: false, browser: false, history: 'all', paused: false };
const agent = { id: 'alpha', name: 'Meera', role: 'Coordinate supplier work', model: 'fixture', scope: 'workspace', revision: 1, settings, editable: true, owner_name: 'Ananya' };
const directory = { role: 'Owner', agents: [agent, { ...agent, id: 'beta', name: 'Tara', role: 'Sales coordination' }], default_model: 'fixture', providers: [], system_prompt: 'System guide' };
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
    await change(document.querySelector('[aria-label="Find an agent"]'), 'Meera');
    assert.equal(document.querySelector('.agent-identity-title h2').textContent, 'Tara');
    assert.equal(button('Open agent Tara'), undefined);
    assert.ok(button('Open agent Meera'));
    assert.equal(writes.length, 0);
    assert.match(document.body.textContent, /Not connected/);
    assert.equal(document.body.textContent.includes('Available around the clock'), false);
    await click(button('Knowledge'));
    assert.match(document.querySelector('[role=tabpanel]').textContent, /Approved knowledge/);
    await update(() => button('Knowledge').dispatchEvent(new window.KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true })));
    assert.equal(document.activeElement, button('Access'));
    assert.equal(button('Access').getAttribute('aria-selected'), 'true');
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async (path, options = {}) => {
    if (options.method === 'POST') writes.push(path);
    return response(read(path));
  } });
});

test('Stop guards duplicate submissions, retains a failure and reconciles the saved request on retry', async () => {
  const writes = []; let complete; let stopped = false;
  await reactFixture(async ({ render, button, click, update }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { AgentWork } = await import('../src/react/agents/work.tsx');
    await render(AgentWork, { agent: 'alpha' });
    assert.match(document.body.textContent, /Check supplier delivery/);
    const link = [...document.querySelectorAll('a')].find(a => a.textContent === 'Open conversation');
    assert.match(link.href, /conversation=channel&message=reply&thread=root/);
    await update(() => { button('Stop request').click(); button('Stop request').click(); });
    assert.equal(writes.length, 1);
    await update(() => complete(response({ error: 'Cannot save cancellation yet' }, 503)));
    assert.match(document.querySelector('[role=alert]').textContent, /Cannot save cancellation yet/);
    assert.match(document.body.textContent, /Check supplier delivery/);
    await click(button('Stop request'));
    await update(() => { stopped = true; complete(response({ ok: true })); });
    assert.equal(writes.length, 2);
    assert.equal(button('Stop request'), undefined);
    assert.ok(writes.every(path => path.endsWith('/chats/channel/messages/reply/cancel')));
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async (path, options = {}) => {
    if (options.method === 'POST') { writes.push(path); return new Promise(resolve => { complete = resolve; }); }
    return response(path.endsWith('/session') ? session : stopped ? empty : { requests: [request], next_before: null });
  } });
});

test('fresh progress reads clear denied work and never start another business request', async () => {
  let denied = false; const writes = [];
  await reactFixture(async ({ render, button, click }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { AgentWork } = await import('../src/react/agents/work.tsx');
    await render(AgentWork, { agent: 'alpha' });
    denied = true; await click(button('Refresh progress'));
    assert.equal(document.body.textContent.includes('Check supplier delivery'), false);
    assert.equal(button('Stop request'), undefined);
    assert.match(document.querySelector('[role=alert]').textContent, /Access denied/);
    assert.equal(writes.length, 0);
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async (path, options = {}) => {
    if (options.method === 'POST') writes.push(path);
    return path.endsWith('/session') ? response(session) : denied ? response({ error: 'Access denied' }, 403) : response({ requests: [request], next_before: null });
  } });
});

test('changing request scope discards a late private response and visible work does not grant Stop', async () => {
  let late;
  await reactFixture(async ({ render, change, update, button }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { AgentWork } = await import('../src/react/agents/work.tsx');
    await render(AgentWork, { agent: 'alpha' });
    await change(document.querySelector('select'), 'visible');
    assert.match(document.body.textContent, /Other visible request/);
    await update(() => late(response({ requests: [{ ...request, summary: 'STALE_PRIVATE_RESPONSE' }], next_before: null })));
    assert.equal(document.body.textContent.includes('STALE_PRIVATE_RESPONSE'), false);
    assert.equal(button('Stop request'), undefined);
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async path => path.endsWith('/session') ? response(session)
    : path.includes('scope=visible') ? response({ requests: [{ ...request, can_cancel: false, summary: 'Other visible request' }], next_before: null })
      : new Promise(resolve => { late = resolve; }) });
});

test('switching selected agent during Open in Chat cannot create or navigate a stale conversation', async () => {
  let chats; const writes = [];
  await reactFixture(async ({ render, button, click, update, navigation }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { AgentsPage } = await import('../src/react/agents/directory.tsx');
    await render(AgentsPage); await click(button('Open in Chat'));
    await click(button('Open agent Tara'));
    await update(() => chats(response({ chats: [] })));
    assert.equal(writes.length, 0); assert.equal(navigation.length, 0);
    assert.equal(document.querySelector('.agent-identity-title h2').textContent, 'Tara');
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async (path, options = {}) => {
    if (options.method === 'POST') writes.push(path);
    if (path.endsWith('/chats')) return new Promise(resolve => { chats = resolve; });
    return response(read(path));
  } });
});
