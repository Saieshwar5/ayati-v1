import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const session = { user: { id: 'owner', email: 'owner@example.test', display_name: 'Ravi Kumar' }, workspaces: [{ id: w, name: 'Mehta Furnishings', role: 'Member' }] };
const users = [{ id: 'owner', name: 'Ravi Kumar', email: 'ravi@example.test' }, { id: 'priya', name: 'Priya Shah', email: 'priya@example.test' }, { id: 'arjun', name: 'Arjun Mehta' }];
const directory = { role: 'Member', agents: [], default_model: 'fixture', system_prompt: 'Use only assigned capabilities.', providers: [] };
const response = data => ({ ok: true, json: async () => data });

async function open(render) {
  const context = await import('../src/workspace-context.ts');
  await context.enterWorkspace();
  const { AgentCreate, newAgent } = await import('../src/react/agents/create/create-flow.tsx');
  const created = [];
  await render(AgentCreate, { start: newAgent(directory), directory, close() {}, created: async id => { created.push(id); } });
  return created;
}
const field = label => [...document.querySelectorAll('label')].find(node => node.textContent.startsWith(label)).querySelector('input,textarea,select');

test('a failed create keeps the whole draft and a double click sends one request', async () => {
  const writes = []; let fail = true;
  await reactFixture(async ({ render, button, click, change, update }) => {
    const created = await open(render);
    assert.equal(button('Continue').disabled, true);
    await change(field('Name'), 'Iris');
    await click(button('Continue'));
    await click(button('Customer support'));
    assert.match(field('Instructions').value, /You are Iris, our customer support assistant/);
    await click(button('Continue'));
    assert.match(document.body.textContent, /Everyone in Mehta Furnishings/);
    assert.match(document.body.textContent, /Ravi Kumar \(you\)ravi@example\.testOwner/);
    await update(() => { button('Create agent').click(); button('Create agent').click(); });
    assert.equal(writes.length, 1); assert.equal(created.length, 0);
    assert.match(document.querySelector('[role=status]').textContent, /Save unavailable/);
    // The draft survives the failure, including earlier steps.
    await click(button('Back')); assert.match(field('Instructions').value, /customer support/);
    await click(button('Back')); assert.equal(field('Name').value, 'Iris'); assert.equal(field('Job title').value, 'Customer support');
    await click(button('Continue')); await click(button('Continue'));
    fail = false; await click(button('Create agent'));
    assert.equal(writes.length, 2); assert.deepEqual(writes[0], writes[1]); assert.equal(created.length, 1);
    assert.equal(writes[1].audience, 'everyone'); assert.deepEqual(writes[1].people, []); assert.deepEqual(writes[1].managers, ['owner']);
    assert.equal(writes[1].role, 'Customer support'); assert.equal(writes[1].revision, 0);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chat-directory')) return response({ users, agents: [], role: 'Member' });
    const body = JSON.parse(options.body); writes.push(body);
    return { ok: !fail, json: async () => fail ? { error: 'Save unavailable' } : { ...body, revision: 1 } };
  } });
});

test('starters only fill an empty box, and selected people and managers are sent', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click, change }) => {
    await open(render);
    await change(field('Name'), 'Ledger'); await change(field('Job title'), 'Books');
    await click(button('Continue'));
    await change(field('Instructions'), 'My own words');
    await click(button('Accounts'));
    assert.equal(field('Instructions').value, 'My own words');
    assert.match(document.body.textContent, /Starters fill an empty box/);
    await click(button('Continue'));
    await click(document.querySelector('input[value=selected]'));
    await click([...document.querySelectorAll('label')].find(node => node.textContent === 'Priya Shah').querySelector('input'));
    await change(field('Add manager'), 'arjun'); await click(button('Add'));
    assert.match(document.body.textContent, /Arjun Mehta/);
    await click(button('Create agent'));
    assert.equal(writes.length, 1);
    assert.equal(writes[0].audience, 'selected'); assert.deepEqual(writes[0].people, ['priya']);
    assert.deepEqual(writes[0].managers, ['owner', 'arjun']); assert.equal(writes[0].role, 'Books');
    assert.equal(writes[0].settings.instructions, 'My own words');
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chat-directory')) return response({ users, agents: [], role: 'Member' });
    writes.push(JSON.parse(options.body)); return response({ ...JSON.parse(options.body), revision: 1 });
  } });
});

test('"See everything it receives" shows the server text for the current draft', async () => {
  const previews = [];
  await reactFixture(async ({ render, button, click, change }) => {
    await open(render);
    await change(field('Name'), 'Iris'); await click(button('Continue'));
    await change(field('Instructions'), 'Be brief.');
    await click(button('See everything Iris receives'));
    assert.match(document.querySelector('dialog').textContent, /Your name is Iris\.\nUse only assigned capabilities\.\nInstructions:\nBe brief\./);
    // Strict Mode mounts twice; the first read is aborted. Every read is for this draft.
    assert.ok(previews.length >= 1);
    for (const body of previews) assert.deepEqual(body, { name: 'Iris', role: '', instructions: 'Be brief.' });
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chat-directory')) return response({ users, agents: [], role: 'Member' });
    const body = JSON.parse(options.body); previews.push(body);
    return response({ prompt: `Your name is ${body.name}.\nUse only assigned capabilities.\nInstructions:\n${body.instructions}` });
  } });
});
