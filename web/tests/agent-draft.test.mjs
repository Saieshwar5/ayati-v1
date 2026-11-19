// New agent keeps what you have so far (schema 139): after each step and each
// change to the skills, so a reload or another device carries on from it.
// Keeping creates nothing; creating or cancelling drops the kept draft.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const session = { user: { id: 'owner', email: 'owner@example.test', display_name: 'Ravi Kumar' }, workspaces: [{ id: w, name: 'Mehta Furnishings', role: 'Member' }] };
const users = [{ id: 'owner', name: 'Ravi Kumar', email: 'ravi@example.test' }, { id: 'priya', name: 'Priya Shah' }];
const directory = { role: 'Member', agents: [], default_model: 'fixture', system_prompt: 'Use only assigned capabilities.', providers: [] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const field = label => [...document.querySelectorAll('label')].find(node => node.textContent.startsWith(label)).querySelector('input,textarea,select');
const short = path => path.replace(`/api/identity/workspaces/${w}`, '');
const saved = { id: '22222222-2222-4222-8222-222222222222', name: 'Iris', role: 'Customer support', avatar: 'robot-07',
  instructions: 'You are Iris.\nNever promise a refund.', audience: 'selected', people: ['priya'], step: 1,
  skills: [{ id: '33333333-3333-4333-8333-333333333333', name: 'order-status', description: 'Use when a customer asks about an order.', steps: '1. Look up the order.' }],
  library: [{ id: 's1', name: 'refund-check', description: 'Use when a refund is asked.' }],
  messages: [{ role: 'person', text: 'It answers customers.' }, { role: 'agent', text: 'Drafted the rules and one skill.' }] };

/** A fake workspace. `draft` is what the server keeps; `writes` are the other saves. */
function server(state, writes = []) {
  return async (path, options = {}) => {
    const route = short(path);
    if (path.endsWith('/session')) return response(session);
    if (route === '/chat-directory') return response({ users, agents: [], role: 'Member' });
    if (route === '/agent-profiles' && !options.body) return response(directory);
    if (route === '/agent-draft' && !options.body) return response({ draft: state.draft ?? null });
    if (route === '/assistant/agent-setup' && !options.body) return response({ available: true, assistant: { id: 'ayati', name: 'Ayati' }, guide: '' });
    if (!options.body) return response({});
    const body = JSON.parse(options.body);
    if (route === '/agent-draft') { state.draft = body; return response({ ok: true }); }
    if (route === '/agent-draft/discard') { if (state.draft?.id === body.id) state.draft = undefined; state.discarded = body.id; return response({ ok: true }); }
    writes.push({ route, body });
    return response(route === '/agent-profiles' ? { ...body, revision: 1 } : { skills: [], editable: true, id: body.id ?? 'x' });
  };
}

test('each step keeps the draft once it is on screen; a failure to keep never blocks; creating drops it', async () => {
  const kept = [], dropped = [], created = []; let fail = false;
  await reactFixture(async ({ render, button, click, change }) => {
    await (await import('../src/workspace-context.ts')).enterWorkspace();
    const { AgentCreate, newAgent } = await import('../src/react/agents/create/create-flow.tsx');
    const start = newAgent(directory);
    await render(AgentCreate, { start, close() {}, created: async id => { created.push(id); },
      keep: async draft => { if (fail) throw new Error('offline'); kept.push(draft); }, discard: async id => { dropped.push(id); } });
    assert.equal(kept.length, 0, 'opening New agent keeps nothing');
    await change(field('Name'), 'Iris');
    assert.equal(kept.length, 0, 'typing keeps nothing');
    await click(button('Continue'));
    assert.equal(kept.length, 1);
    assert.deepEqual({ ...kept[0], avatar: '' }, { id: start.id, name: 'Iris', role: '', avatar: '', instructions: '', audience: 'everyone', people: [], step: 1, skills: [], library: [], messages: [] });
    assert.match(kept[0].avatar, /^robot-\d\d$/);
    await change(field('Ground rules'), 'Be kind.');
    fail = true;
    await click(button('Continue'));
    assert.match(document.body.textContent, /Your progress couldn’t be saved, so it stays in this tab only\./);
    assert.match(document.body.textContent, /Everyone in Mehta Furnishings/, 'the next step still opens');
    fail = false;
    await click(button('Back'));
    assert.equal(kept.at(-1).instructions, 'Be kind.');
    assert.equal(kept.at(-1).step, 1);
    assert.equal(document.body.textContent.includes('couldn’t be saved'), false);
    await click(button('Continue'));
    await click(document.querySelector('input[value=selected]'));
    await click([...document.querySelectorAll('label')].find(node => node.textContent === 'Priya Shah').querySelector('input'));
    await click(button('Create agent'));
    assert.deepEqual(dropped, [start.id], 'the kept draft goes once the agent exists');
    assert.deepEqual(created, [start.id]);
  }, { url, fetch: server({}) });
});

test('New agent carries on from a kept draft: its step, rules, skills, people and the chat so far', async () => {
  const kept = [];
  await reactFixture(async ({ render, button, click }) => {
    await (await import('../src/workspace-context.ts')).enterWorkspace();
    const { AgentCreate, newAgent } = await import('../src/react/agents/create/create-flow.tsx');
    const { agentFrom } = await import('../src/react/agents/create/saved-draft.ts');
    await render(AgentCreate, { start: agentFrom(saved, newAgent(directory)), resume: saved, close() {}, created: async () => {}, keep: async draft => { kept.push(draft); } });
    assert.equal(field('Ground rules').value, 'You are Iris.\nNever promise a refund.');
    assert.match(document.querySelector('.draft-skills').textContent, /order-status/);
    assert.match(document.querySelector('.skill-chips').textContent, /refund-check/);
    assert.match(document.querySelector('.setup-chat-messages').textContent, /It answers customers\..*Drafted the rules and one skill\./s);
    // Removing a skill is kept too, with the rest unchanged.
    await click(button('Remove order-status'));
    assert.deepEqual(kept.at(-1), { ...saved, skills: [] });
    await click(button('Continue'));
    assert.equal(document.querySelector('input[value=selected]').checked, true);
    assert.equal([...document.querySelectorAll('label')].find(node => node.textContent === 'Priya Shah').querySelector('input').checked, true);
    await click(button('Back')); await click(button('Back'));
    assert.equal(field('Name').value, 'Iris'); assert.equal(field('Job title').value, 'Customer support');
  }, { url, fetch: server({}) });
});

test('the Agents page offers the agent you had started; Cancel drops it and creates nothing', async () => {
  const state = { draft: saved }, writes = [];
  await reactFixture(async ({ render, button, click }) => {
    await (await import('../src/workspace-context.ts')).enterWorkspace();
    const { AgentsPage } = await import('../src/react/agents/directory.tsx');
    await render(AgentsPage);
    const row = button('Continue setting up Iris');
    assert.match(row.textContent, /IrisNot created yet · Continue/);
    await click(row);
    assert.equal(field('Ground rules').value, 'You are Iris.\nNever promise a refund.', 'it opens where you left it');
    assert.equal(button('Continue setting up Iris'), undefined);
    await click(button('Cancel'));
    assert.equal(state.discarded, saved.id);
    assert.equal(button('Continue setting up Iris'), undefined, 'the row goes with the draft');
    assert.equal(writes.length, 0, 'no agent and no skill were saved');
    // With nothing kept, New agent starts fresh.
    await click(button('＋ New agent'));
    assert.equal(field('Name').value, '');
  }, { url, fetch: server(state, writes) });
});
