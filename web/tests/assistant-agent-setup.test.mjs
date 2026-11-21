// A member's own assistant is the helper in the setup chat (schema 139): it is
// asked in its own room, its drafts are read once it has answered, and nothing
// is saved until the person creates or changes the agent themselves.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const session = { user: { id: 'owner', email: 'owner@example.test', display_name: 'Ravi Kumar' }, workspaces: [{ id: w, name: 'Mehta Furnishings', role: 'Member' }] };
const users = [{ id: 'owner', name: 'Ravi Kumar', email: 'ravi@example.test' }];
const directory = { role: 'Member', agents: [], default_model: 'fixture', system_prompt: 'Use only assigned capabilities.', providers: [] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const help = { available: true, assistant: { id: 'ayati', name: 'Ayati', avatar: 'robot-01' }, guide: 'ASSISTANT GUIDE' };
const drafted = { state: 'complete', reply: 'Drafted the rules and one skill.', ground_rules: 'You are Iris.\nNever promise a refund.',
  skills: [{ name: 'order-status', description: 'Use when a customer asks where an order is', steps: '1. Ask for the order number.\n2. Look up the order.', text: '' }],
  remove: [], reuse: [], missing_apps: ['Shopify'], notes: [], job_title: 'Customer support', used: ['Priya handles upset customers'] };
const field = label => [...document.querySelectorAll('label')].find(node => node.textContent.startsWith(label)).querySelector('input,textarea,select');
const composer = () => document.querySelector('#setup-chat-input');
const short = path => path.replace(`/api/identity/workspaces/${w}`, '');

/** A fake workspace: `turns` are what reading a turn returns, in order; the last repeats. */
function server({ asked = [], writes = [], reads = [], turns = [drafted], assistant = help, refuse = 0 } = {}) {
  return async (path, options = {}) => {
    const route = short(path);
    if (path.endsWith('/session')) return response(session);
    if (route === '/chat-directory') return response({ users, agents: [], role: 'Member' });
    if (route === '/assistant/agent-setup' && !options.body) return response(assistant);
    if (route === '/skill-writer' && !options.body) return response({ available: true, guide: 'WRITER GUIDE' });
    if (route.startsWith('/assistant/agent-setup/')) { reads.push(route); return response(turns[Math.min(reads.length, turns.length) - 1]); }
    const body = JSON.parse(options.body);
    if (route === '/assistant/agent-setup') {
      asked.push(body);
      return asked.length <= refuse ? response({ error: 'Your assistant is paused' }, 400) : response({ ok: true, chat: 'room', reply: `r${asked.length}` });
    }
    writes.push({ route, body });
    if (route === '/skill-writer') return response({ reply: 'The writer answered.', ground_rules: null, skills: [], remove: [], reuse: [], missing_apps: [], notes: [] });
    return response({});
  };
}
async function openStep2(render, button, click, change, dom) {
  // The drafts are read on a timer: run it by hand.
  const timers = { cleared: [] };
  dom.window.setInterval = fn => { timers.tick = fn; return 7; }; dom.window.clearInterval = id => { timers.cleared.push(id); };
  await (await import('../src/workspace-context.ts')).enterWorkspace();
  const { AgentCreate, newAgent } = await import('../src/react/agents/create/create-flow.tsx');
  await render(AgentCreate, { start: newAgent(directory), directory, close() {}, created: async () => {} });
  await change(field('Name'), 'Iris'); await click(button('Continue'));
  return timers;
}

test('in New agent your assistant is the helper: asked once, its drafts fill the step, and nothing is saved', async () => {
  const asked = [], writes = [], reads = [];
  await reactFixture(async ({ render, button, click, change, update, dom }) => {
    const timers = await openStep2(render, button, click, change, dom);
    const chat = document.querySelector('[aria-label="Chat with Ayati"]');
    assert.ok(chat, 'the chat is with your assistant, not with the agent that does not exist yet');
    assert.match(chat.querySelector('.setup-chat-header').textContent, /Your assistant · helping you set up Iris · nothing is saved until you create it/);
    assert.match(chat.textContent, /Tell me what Iris should do/);
    assert.match(chat.querySelector('.setup-chat-guide').textContent, /What Ayati is told while setting up.*ASSISTANT GUIDE/s);

    await change(composer(), ' Iris answers customers about orders. ');
    await update(() => { button('Send').click(); button('Send').click(); });
    assert.equal(asked.length, 1, 'one turn despite a double click');
    assert.match(asked[0].id, /^[0-9a-f-]{36}$/);
    assert.deepEqual(asked[0].setup, { name: 'Iris', role: '', connections: [], ground_rules: '', skills: [],
      messages: [{ role: 'person', text: 'Iris answers customers about orders.' }] });
    // Your words show at once and the box is free while it writes.
    assert.equal(chat.querySelector('.setup-chat-message.person').textContent, 'Iris answers customers about orders.');
    assert.match(chat.querySelector('[role=status]').textContent, /Ayati is writing…/);
    assert.equal(composer().value, '');
    assert.equal(button('Send').disabled, true);
    assert.equal(button('Customer support'), undefined, 'the starters go once you have said something');

    await update(() => { void timers.tick(); });
    assert.equal(field('Ground rules').value, '', 'nothing is shown while it works');
    await update(() => { void timers.tick(); });
    assert.deepEqual(reads, ['/assistant/agent-setup/r1', '/assistant/agent-setup/r1']);
    assert.match(chat.querySelector('.setup-chat-messages').textContent, /Drafted the rules and one skill\./);
    assert.equal(field('Ground rules').value, 'You are Iris.\nNever promise a refund.');
    assert.match(document.querySelector('.draft-skills').textContent, /order-status NewUse when a customer asks where an order is2 steps/);
    assert.match(chat.querySelector('[aria-label="What Ayati used"]').textContent, /Priya handles upset customers/);
    assert.match(chat.querySelector('.setup-chat-notes').textContent, /Iris will need Shopify\. Connect it on its Connections tab once it is created\./);
    assert.ok(button('Ask Ayati about order-status'));
    // It suggested a job title, because the agent had none.
    await click(button('Back'));
    assert.equal(field('Job title').value, 'Customer support');
    assert.equal(writes.length, 0, 'nothing is saved, and the writer is not asked');
  }, { url, fetch: server({ asked, writes, reads, turns: [{ state: 'queued' }, drafted] }) });
});

test('a turn that fails, is stopped or is refused gives your words back', async () => {
  const asked = [], reads = [];
  await reactFixture(async ({ render, button, click, change, update, dom }) => {
    const timers = await openStep2(render, button, click, change, dom);
    const chat = document.querySelector('[aria-label="Chat with Ayati"]');
    // Refused before it starts: the reason is shown and the words stay.
    await change(composer(), 'Answer order questions.');
    await click(button('Send'));
    assert.match(chat.querySelector('[role=alert]').textContent, /Your assistant is paused/);
    assert.equal(composer().value, 'Answer order questions.');
    // It could not finish.
    await click(button('Send'));
    assert.equal(composer().value, '');
    await update(() => { void timers.tick(); });
    assert.match(chat.querySelector('[role=alert]').textContent, /Your assistant couldn’t answer\. Try again, or write it yourself\./);
    assert.equal(composer().value, 'Answer order questions.');
    assert.equal(chat.querySelectorAll('.setup-chat-message.person').length, 0);
    assert.equal(field('Ground rules').value, '');
    // Stopped while it writes: no error, and what was typed since is kept.
    await click(button('Send'));
    await change(composer(), 'One more thing.');
    await click(button('Stop'));
    assert.equal(chat.querySelector('[role=status]'), null);
    assert.equal(chat.querySelector('[role=alert]'), null);
    assert.equal(composer().value, 'One more thing.');
    assert.equal(button('Send').disabled, false);
    assert.equal(asked.length, 3);
  }, { url, fetch: server({ asked, reads, turns: [{ state: 'failed' }], refuse: 1 }) });
});

test('leaving the step stops reading; without an assistant the skill writer still helps', async () => {
  const reads = [], writes = [];
  await reactFixture(async ({ render, button, click, change, update, dom }) => {
    const timers = await openStep2(render, button, click, change, dom);
    await change(composer(), 'Answer order questions.');
    await click(button('Send'));
    const before = timers.cleared.length;
    await render(() => null);
    assert.ok(timers.cleared.length > before, 'the timer is cleared with the screen');
    await update(() => { void timers.tick(); });
    assert.equal(reads.length <= 1, true, 'a late read is not repeated');
  }, { url, fetch: server({ reads, turns: [{ state: 'queued' }] }) });
  await reactFixture(async ({ render, button, click, change, dom }) => {
    await openStep2(render, button, click, change, dom);
    assert.ok(document.querySelector('[aria-label="Chat with Iris"]'), 'the writer speaks as the agent, as before');
    await change(composer(), 'Answer order questions.');
    await click(button('Send'));
    assert.deepEqual(writes.map(write => write.route), ['/skill-writer']);
    assert.match(document.querySelector('.setup-chat-messages').textContent, /The writer answered\./);
  }, { url, fetch: server({ writes, assistant: { available: false } }) });
});

test('on an agent’s page the chat is with your assistant, which proposes changes you apply yourself', async () => {
  const settings = { provider: '', instructions: 'You are Atlas.', escalation: '', avatar: 'robot-03', memory: false, browser: false, history: 'all', paused: false };
  const agent = { id: 'atlas', name: 'Atlas', role: 'Orders assistant', model: 'fixture', audience: 'everyone', revision: 5, settings, editable: true, owner_id: 'owner', people: [] };
  const skill = { id: 's1', name: 'refund-check', description: 'Use when a refund is asked', shared: false, owner_id: 'owner', owner_name: 'Ravi Kumar', version: 2, license: null, has_scripts: false, updated_ms: 1, files: 1, agents: ['atlas'] };
  const files = [{ path: 'SKILL.md', text: '---\nname: refund-check\ndescription: Use when a refund is asked\n---\n1. Ask Meera above 5000.\n' }];
  const asked = [], writes = [];
  const answer = { ...drafted, ground_rules: null, missing_apps: [], job_title: null, used: [],
    skills: [{ name: 'refund-check', description: 'Use when a refund is asked', steps: '1. Ask Meera above 3000.', text: '' }] };
  const base = server({ asked, writes, turns: [answer] });
  await reactFixture(async ({ render, button, click, change, update, dom }) => {
    let tick; dom.window.setInterval = fn => { tick = fn; return 7; }; dom.window.clearInterval = () => {};
    await (await import('../src/workspace-context.ts')).enterWorkspace();
    const { AgentWorkspace } = await import('../src/react/agents/workspace.tsx');
    await render(AgentWorkspace, { agent, directory: { ...directory, agents: [agent] }, revision: 0, back() {}, guide() {}, control() {}, openChat() {}, busy: false,
      saved: async () => {} });
    assert.equal(document.querySelector('[aria-label="Change with Ayati"]'), null, 'the chat opens when asked for');
    await click(button('Ask Ayati'));
    const chat = document.querySelector('[aria-label="Change with Ayati"]');
    assert.ok(chat);
    assert.match(chat.textContent, /Your assistant · only you see this chat/);
    await change(composer(), 'Refunds above ₹3,000 now go to Meera.');
    await click(button('Send'));
    assert.equal(asked[0].setup.agent, 'atlas');
    assert.deepEqual(asked[0].setup.skills, [{ name: 'refund-check', description: 'Use when a refund is asked', steps: '1. Ask Meera above 5000.', saved: true }]);
    await update(() => { void tick(); });
    assert.match(document.querySelector('[aria-label="Proposed changes"]').textContent, /− 1\. Ask Meera above 5000\.\n\+ 1\. Ask Meera above 3000\./);
    assert.equal(writes.length, 0, 'nothing changes until you apply it');
    await click(button('Save as version 3'));
    assert.deepEqual(writes.map(write => write.route), ['/skills/s1/versions']);
  }, { url, fetch: async (path, options = {}) => {
    const route = short(path);
    if (route === '/skills/s1/versions') { writes.push({ route, body: JSON.parse(options.body) }); return response({ ...skill, version: 3, files }); }
    if (route === '/skills/s1') return response({ ...skill, files, hash: 'h', shown_version: 2, versions: [] });
    if (route === '/agent-profiles/atlas/skills') return response({ skills: [skill], editable: true });
    if (route.endsWith('/knowledge')) return response({ notes: [] });
    if (route.includes('/library-always/')) return response({ items: [], given: [], hidden: 0, chosen: 0, editable: false, most: 5, budget: 8000 });
    if (!options.body && !route.startsWith('/assistant/agent-setup') && route !== '/skill-writer' && route !== '/chat-directory' && !path.endsWith('/session')) return response({});
    return base(path, options);
  } });
});
