import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const session = { user: { id: 'owner', email: 'owner@example.test', display_name: 'Ravi Kumar' }, workspaces: [{ id: w, name: 'Store', role: 'Member' }] };
const settings = { provider: '', instructions: '## Responsibility\nAnswer order questions.', escalation: '', avatar: 'robot-03', memory: true, browser: false, history: 'all', paused: false };
const agent = { id: 'atlas', name: 'Atlas', role: 'Orders assistant', model: 'fixture', audience: 'everyone', revision: 5, settings, editable: true, owner_id: 'owner', owner_name: 'Ravi Kumar', people: [], managers: ['owner'] };
const directory = { role: 'Member', agents: [agent], default_model: 'fixture', providers: [], system_prompt: 'System guide' };
const users = [{ id: 'owner', name: 'Ravi Kumar' }, { id: 'priya', name: 'Priya Shah' }];
const response = data => ({ ok: true, json: async () => data });
function read(path) {
  if (path.endsWith('/session')) return session;
  if (path.endsWith('/chat-directory')) return { users, agents: [], role: 'Member' };
  if (path.includes('/requests')) return { requests: [], next_before: null };
  if (path.endsWith('/control')) return { can_manage: true, connections: [], rules: [], actions: [], servers: [], conversations: [], delegations: [], viewer: 'owner' };
  return {};
}
async function open(render, props = {}) {
  const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
  const { AgentWorkspace } = await import('../src/react/agents/workspace.tsx');
  const calls = { saved: 0 };
  await render(AgentWorkspace, { agent, directory, revision: 0, back() {}, guide() {}, control() {}, openChat() {}, busy: false,
    section: 'Overview', selectSection() {}, saved: async () => { calls.saved++; }, ...props });
  return calls;
}

test('overview shows what it does and who can talk to it, without the old placeholders', async () => {
  await reactFixture(async ({ render }) => {
    await open(render);
    const text = document.body.textContent;
    assert.match(text, /AboutAnswer order questions\./);
    assert.match(text, /Orders assistant · owned by Ravi Kumar/);
    assert.match(text, /Who can talk to itEveryone in the workspace/);
    assert.match(text, /ManagersRavi Kumar/);
    assert.equal(text.includes('Not connected'), false);
    assert.equal(text.includes('Try a sample task'), false);
  }, { url, fetch: async path => response(read(path)) });
});

test('pause from the ⋯ menu saves once and leaves the people lists alone', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click, update }) => {
    const calls = await open(render);
    await click(button('More actions for Atlas'));
    await update(() => { button('Pause').click(); });
    assert.equal(writes.length, 1);
    assert.equal(writes[0].settings.paused, true); assert.equal(writes[0].revision, 5);
    assert.equal(writes[0].people, undefined); assert.equal(writes[0].managers, undefined);
    assert.equal(calls.saved, 1);
    assert.match(document.body.textContent, /Atlas is paused/);
  }, { url, fetch: async (path, options = {}) => {
    if (options.body) { const body = JSON.parse(options.body); writes.push(body); return response({ ...body, revision: 6 }); }
    return response(read(path));
  } });
});

test('transfer owner sends the chosen person with the current revision', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click, change }) => {
    await open(render);
    await click(button('More actions for Atlas'));
    await click(button('Transfer owner…'));
    assert.equal(button('Transfer owner').disabled, true);
    await change(document.querySelector('dialog select'), 'priya');
    await click(button('Transfer owner'));
    assert.deepEqual(writes, [{ path: `/api/identity/workspaces/${w}/agent-profiles/atlas/owner`, body: { owner: 'priya', revision: 5 } }]);
    assert.match(document.body.textContent, /Priya Shah now owns Atlas/);
  }, { url, fetch: async (path, options = {}) => {
    if (options.body) { writes.push({ path, body: JSON.parse(options.body) }); return response({ owner: 'priya', revision: 6 }); }
    return response(read(path));
  } });
});

test('people who cannot change the agent get no ⋯ menu or Change links', async () => {
  await reactFixture(async ({ render, button }) => {
    await open(render, { agent: { ...agent, editable: false } });
    assert.equal(button('More actions for Atlas'), undefined);
    assert.equal(button('Change who can talk to it'), undefined);
    assert.ok(button('Open in Chat'));
  }, { url, fetch: async path => response(read(path)) });
});
