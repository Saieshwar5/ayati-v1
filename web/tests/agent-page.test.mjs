import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const session = { user: { id: 'owner', email: 'owner@example.test', display_name: 'Ravi Kumar' }, workspaces: [{ id: w, name: 'Store', role: 'Member' }] };
const settings = { provider: '', instructions: '## Responsibility\nAnswer order questions.', escalation: '', avatar: 'robot-03', memory: true, browser: false, history: 'all', paused: false };
const agent = { id: 'atlas', name: 'Atlas', role: 'Orders assistant', model: 'fixture', audience: 'everyone', revision: 5, settings, editable: true, owner_id: 'owner', owner_name: 'Ravi Kumar', people: [] };
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
    assert.match(text, /OwnerRavi Kumar/);
    assert.ok(document.querySelector('[aria-label="Change owner"]'), 'owners and admins change owners from Overview');
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
    assert.equal(writes[0].people, undefined); assert.equal('managers' in writes[0], false);
    assert.equal(calls.saved, 1);
    assert.match(document.body.textContent, /Atlas is paused/);
  }, { url, fetch: async (path, options = {}) => {
    if (options.body) { const body = JSON.parse(options.body); writes.push(body); return response({ ...body, revision: 6 }); }
    return response(read(path));
  } });
});

// Owners are equal: any owner adds or removes others; the last one stays (schema 122).
const ravi = { id: 'owner', name: 'Ravi Kumar' }, priya = { id: 'priya', name: 'Priya Shah' };
function ownersServer(writes, fail) {
  let owners = [ravi], revision = 5;
  return async (path, options = {}) => {
    if (!options.body) return response(read(path));
    const body = JSON.parse(options.body);
    writes.push({ path, body });
    if (fail?.(body)) return { ok: false, status: 409, json: async () => ({ error: 'Agent changed; reload before saving' }) };
    if (body.add) owners = [...owners.filter(o => o.id !== body.add), body.add === 'priya' ? priya : ravi];
    else owners = owners.filter(o => o.id !== body.remove);
    revision++;
    return response({ owners, revision });
  };
}

test('every owner is listed in the header and Overview', async () => {
  await reactFixture(async ({ render }) => {
    await open(render, { agent: { ...agent, owners: [ravi, priya], is_owner: true } });
    assert.match(document.body.textContent, /Orders assistant · owned by Ravi Kumar and Priya Shah/);
    assert.match(document.body.textContent, /OwnersRavi Kumar and Priya Shah/);
  }, { url, fetch: async path => response(read(path)) });
});

test('owners add and remove each other with the latest revision; the last owner stays', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click, change, update }) => {
    const calls = await open(render, { agent: { ...agent, owners: [ravi], is_owner: true } });
    await click(button('Change owner'));
    assert.equal(document.querySelector('dialog h2').textContent, 'Atlas · Owners');
    assert.equal(button('Stop being an owner').disabled, true, 'the last owner cannot be removed');
    assert.match(document.querySelector('dialog').textContent, /Atlas needs at least one owner/);
    assert.match(document.querySelector('dialog').textContent, /Every owner can change Atlas, approve its actions and see all its work/);
    const select = document.querySelector('dialog select');
    assert.deepEqual([...select.options].map(option => option.value), ['', 'priya'], 'existing owners are not offered');
    assert.equal(button('Add owner').disabled, true);
    await change(select, 'priya');
    // A double click adds once.
    await update(() => { button('Add owner').click(); button('Add owner').click(); });
    assert.deepEqual(writes, [{ path: `/api/identity/workspaces/${w}/agent-profiles/atlas/owners`, body: { add: 'priya', revision: 5 } }]);
    assert.match(document.querySelector('dialog').textContent, /Priya Shah is now an owner of Atlas/);
    assert.equal(button('Stop being an owner').disabled, false);
    // The next change carries the revision the server returned.
    await click(button('Remove Priya Shah'));
    assert.deepEqual(writes[1].body, { remove: 'priya', revision: 6 });
    assert.equal(calls.saved, 0, 'the page reloads once, when the dialog closes');
    await click(button('Done'));
    assert.equal(calls.saved, 1);
    assert.match(document.body.textContent, /Priya Shah is no longer an owner of Atlas/);
  }, { url, fetch: ownersServer(writes) });
});

test('a failed change keeps the dialog and the chosen person', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click, change }) => {
    const calls = await open(render, { agent: { ...agent, owners: [ravi], is_owner: true } });
    await click(button('Change owner'));
    await change(document.querySelector('dialog select'), 'priya');
    await click(button('Add owner'));
    assert.match(document.querySelector('dialog').textContent, /Agent changed; reload before saving/);
    assert.equal(document.querySelector('dialog select').value, 'priya');
    await click(button('Done'));
    assert.equal(calls.saved, 0, 'nothing changed, so nothing reloads');
  }, { url, fetch: ownersServer(writes, () => true) });
});

test('an owner who stops being one is taken back to the reloaded page at once', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click, change }) => {
    const calls = await open(render, { agent: { ...agent, owners: [ravi], is_owner: true } });
    await click(button('Change owner'));
    await change(document.querySelector('dialog select'), 'priya');
    await click(button('Add owner'));
    await click(button('Stop being an owner'));
    assert.deepEqual(writes[1].body, { remove: 'owner', revision: 6 });
    assert.equal(calls.saved, 1);
    assert.equal(document.querySelector('dialog'), null);
    assert.match(document.body.textContent, /You stopped being an owner of Atlas/);
  }, { url, fetch: ownersServer(writes) });
});

test('an agent whose last owner left says so, cannot resume, and an admin gives it an owner', async () => {
  const writes = [];
  const ownerless = { ...agent, owner_id: null, owner_name: 'Former member', owners: [], needs_owner: true, settings: { ...settings, paused: true } };
  await reactFixture(async ({ render, button, click, change }) => {
    const calls = await open(render, { agent: ownerless });
    assert.match(document.body.textContent, /Needs an owner/);
    assert.match(document.body.textContent, /Orders assistant · its last owner left the workspace/);
    assert.match(document.querySelector('[role=note]').textContent, /Atlas’s last owner left the workspace, so it is paused and takes no work/);
    assert.match(document.body.textContent, /OwnerNone: its last owner left/);
    // Without an owner it can't be resumed: giving it one comes first.
    await click(button('More actions for Atlas'));
    assert.equal(button('Resume'), undefined);
    await click(button('Give it an owner…'));
    assert.equal(document.querySelector('dialog h2').textContent, 'Give Atlas an owner');
    assert.match(document.querySelector('dialog').textContent, /starts again if it was working/);
    await change(document.querySelector('dialog select'), 'priya');
    await click(button('Add owner'));
    assert.deepEqual(writes, [{ path: `/api/identity/workspaces/${w}/agent-profiles/atlas/owners`, body: { add: 'priya', revision: 5 } }]);
    await click(button('Done'));
    assert.equal(calls.saved, 1);
    assert.match(document.body.textContent, /Priya Shah is now an owner of Atlas/);
  }, { url, fetch: async (path, options = {}) => {
    if (options.body) { writes.push({ path, body: JSON.parse(options.body) }); return response({ owners: [priya], revision: 6 }); }
    return response(read(path));
  } });
});

test('people who are not admins are told an admin must give it an owner', async () => {
  await reactFixture(async ({ render, button }) => {
    await open(render, { agent: { ...agent, owner_id: null, needs_owner: true, editable: false, settings: { ...settings, paused: true } } });
    assert.match(document.querySelector('[role=note]').textContent, /An admin needs to give it a new owner/);
    assert.equal(button('Give it an owner'), undefined);
  }, { url, fetch: async path => response(read(path)) });
});

test('people who cannot change the agent get no ⋯ menu or Change links', async () => {
  await reactFixture(async ({ render, button }) => {
    await open(render, { agent: { ...agent, editable: false } });
    assert.equal(button('More actions for Atlas'), undefined);
    assert.equal(button('Change who can talk to it'), undefined);
    assert.ok(button('Open in Chat'));
  }, { url, fetch: async path => response(read(path)) });
});

test('the Instructions tab shows everything the saved agent receives, from its own apps and skills', async () => {
  const previews = [];
  await reactFixture(async ({ render, button, click }) => {
    await open(render, { section: 'Instructions' });
    assert.match(document.body.textContent, /Cuckoo system prompt Always includedSystem guide/);
    await click(button('See everything Atlas receives'));
    const dialog = document.querySelector('dialog');
    assert.match(dialog.textContent, /SkillsIncluded · It has skills/);
    assert.match(dialog.textContent, /quote-builder: Use when a customer asks for a price/);
    assert.ok(previews.length >= 1);
    for (const body of previews) assert.deepEqual(body, { name: 'Atlas', role: 'Orders assistant', instructions: settings.instructions, agent: 'atlas', skills: [] });
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/agent-profiles/prompt')) {
      previews.push(JSON.parse(options.body));
      return response({ prompt: 'Your name is Atlas.', version: '2026-11-08', instructions: '', skills: [{ name: 'quote-builder', description: 'Use when a customer asks for a price' }],
        sections: [{ id: 'skills', title: 'Skills', state: 'included', reason: 'It has skills', text: 'Skills you can use.' }] });
    }
    if (path.endsWith('/skills')) return response({ skills: [], editable: true });
    if (path.endsWith('/knowledge')) return response({ notes: [] });
    if (path.includes('/library-always/')) return response({ items: [], given: [], hidden: 0, chosen: 0, editable: false, most: 5, budget: 8000 });
    return response(read(path));
  } });
});
