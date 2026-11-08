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
    assert.deepEqual([...document.querySelectorAll('.agent-create-steps li')].map(step => step.textContent.slice(1)),
      ['Name and picture', 'Connectors', 'Skills', 'People', 'Controls']);
    assert.match(document.body.textContent, /No connectors yet/);
    await click(button('Continue'));
    await click(button('Customer support'));
    assert.match(field('Ground rules').value, /You are Iris, our customer support assistant/);
    assert.match(document.querySelector('.agent-system-bar').textContent, /Cuckoo system prompt · the same for every agent/);
    await click(button('Continue'));
    assert.match(document.body.textContent, /Everyone in Mehta Furnishings/);
    assert.match(document.body.textContent, /Ravi Kumar \(you\)ravi@example\.testOwner/);
    await click(button('Continue'));
    assert.match(document.body.textContent, /No apps chosen, so Iris can’t act in any app/);
    await update(() => { button('Create agent').click(); button('Create agent').click(); });
    assert.equal(writes.length, 1); assert.equal(created.length, 0);
    assert.match(document.querySelector('[role=status]').textContent, /Save unavailable/);
    // The draft survives the failure, including earlier steps.
    await click(button('Back')); await click(button('Back')); assert.match(field('Ground rules').value, /customer support/);
    await click(button('Back')); await click(button('Back')); assert.equal(field('Name').value, 'Iris'); assert.equal(field('Job title').value, 'Customer support');
    for (let next = 0; next < 4; next++) await click(button('Continue'));
    fail = false; await click(button('Create agent'));
    assert.equal(writes.length, 2); assert.deepEqual(writes[0], writes[1]); assert.equal(created.length, 1);
    assert.equal(writes[1].audience, 'everyone'); assert.deepEqual(writes[1].people, []); assert.deepEqual(writes[1].managers, ['owner']);
    assert.equal(writes[1].role, 'Customer support'); assert.equal(writes[1].revision, 0);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chat-directory')) return response({ users, agents: [], role: 'Member' });
    if (path.endsWith('/control/available')) return response({ connections: [], execution_available: true });
    const body = JSON.parse(options.body); writes.push(body);
    return { ok: !fail, json: async () => fail ? { error: 'Save unavailable' } : { ...body, revision: 1 } };
  } });
});

test('starters only fill an empty box, and selected people and managers are sent', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click, change }) => {
    await open(render);
    await change(field('Name'), 'Ledger'); await change(field('Job title'), 'Books');
    await click(button('Continue')); await click(button('Continue'));
    await change(field('Ground rules'), 'My own words');
    await click(button('Accounts'));
    assert.equal(field('Ground rules').value, 'My own words');
    assert.match(document.body.textContent, /Starters fill an empty box/);
    await click(button('Continue'));
    await click(document.querySelector('input[value=selected]'));
    await click([...document.querySelectorAll('label')].find(node => node.textContent === 'Priya Shah').querySelector('input'));
    await change(field('Add manager'), 'arjun'); await click(button('Add'));
    assert.match(document.body.textContent, /Arjun Mehta/);
    await click(button('Continue'));
    await click(button('Create agent'));
    assert.equal(writes.length, 1);
    assert.equal(writes[0].audience, 'selected'); assert.deepEqual(writes[0].people, ['priya']);
    assert.deepEqual(writes[0].managers, ['owner', 'arjun']); assert.equal(writes[0].role, 'Books');
    assert.equal(writes[0].settings.instructions, 'My own words');
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chat-directory')) return response({ users, agents: [], role: 'Member' });
    if (path.endsWith('/control/available')) return response({ connections: [], execution_available: true });
    writes.push(JSON.parse(options.body)); return response({ ...JSON.parse(options.body), revision: 1 });
  } });
});

test('"See everything it receives" shows the Cuckoo system prompt in sections for the current draft', async () => {
  const previews = [];
  const sections = [
    { id: 'identity', title: 'Who it is and safety', state: 'included', reason: 'Always', text: 'Your name is Iris.\nUse only assigned capabilities.' },
    { id: 'business_actions', title: 'Business actions and approvals', state: 'not_included', reason: 'Added when an app is switched on', text: 'Use Cuckoo control tools.' },
    { id: 'shared', title: 'Shared conversations', state: 'conversation', reason: 'Added in channels and groups', text: 'This conversation is shared.' },
  ];
  await reactFixture(async ({ render, button, click, change }) => {
    await open(render);
    await change(field('Name'), 'Iris'); await click(button('Continue')); await click(button('Continue'));
    await change(field('Ground rules'), 'Be brief.');
    await click(button('See everything Iris receives'));
    const dialog = document.querySelector('dialog');
    assert.match(dialog.textContent, /1 · Cuckoo system promptSame for every agent · 2026-11-08/);
    assert.match(dialog.textContent, /Who it is and safetyIncluded · Always/);
    assert.match(dialog.textContent, /Business actions and approvalsNot included · Added when an app is switched on/);
    assert.match(dialog.textContent, /Shared conversationsIn some conversations · Added in channels and groups/);
    assert.match(dialog.textContent, /2 · Iris’s instructionsInstructions:\nBe brief\./);
    assert.match(dialog.textContent, /No skills yet\./);
    // Strict Mode mounts twice; the first read is aborted. Every read is for this draft.
    assert.ok(previews.length >= 1);
    for (const body of previews) assert.deepEqual(body, { name: 'Iris', role: '', instructions: 'Be brief.', apps: false, skills: [] });
    await click(button('Copy as text'));
    assert.match(dialog.textContent, /Copy isn’t available here/);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chat-directory')) return response({ users, agents: [], role: 'Member' });
    if (path.endsWith('/control/available')) return response({ connections: [], execution_available: true });
    const body = JSON.parse(options.body); previews.push(body);
    return response({ prompt: `Your name is ${body.name}.\nInstructions:\n${body.instructions}`, version: '2026-11-08', sections,
      instructions: `\nInstructions:\n${body.instructions}`, skills: [] });
  } });
});

test('Connectors switches apps on; Controls, last, saves only the stricter choices', async () => {
  const writes = [];
  const connection = { id: 'books', server: 'books', name: 'Zoho Books', account: 'Accounts', revision: 1, on: false, operations: [
    { id: 'invoice.read', title: 'Read invoices', effect: 'read', meaning: 'read', workspace_mode: 'allow', needs_other: false, agent_mode: null, mode: 'allow', revision: 0 },
    { id: 'invoice.send', title: 'Send an invoice email', effect: 'write', meaning: 'sends_outside', workspace_mode: 'ask', needs_other: true, agent_mode: null, mode: 'ask', revision: 0 },
    { id: 'invoice.delete', title: 'Delete an invoice', effect: 'write', meaning: 'delete', workspace_mode: 'block', needs_other: false, agent_mode: null, mode: 'block', revision: 0 }] };
  await reactFixture(async ({ render, button, click, change }) => {
    await open(render);
    await change(field('Name'), 'Ledger'); await click(button('Continue'));
    const app = document.querySelector('.agent-apps li');
    assert.match(app.textContent, /Zoho BooksAccounts · 3 actionsReadsSends outsideDeletesRead invoices · Send an invoice email · Delete an invoice/);
    const use = app.querySelector('[role=switch]');
    assert.equal(use.checked, false);
    await click(use);
    await click(button('Continue'));
    await change(field('Ground rules'), 'Keep the books tidy.'); await click(button('Continue'));
    await click(button('Continue'));
    const zoho = document.querySelector('[aria-label="Zoho Books"]');
    assert.match(zoho.textContent, /Send an invoice emailSends outside · admin limit: Ask first/);
    // Looser than the admin's rule cannot be chosen; stricter can.
    const send = zoho.querySelector('[aria-label="Send an invoice email rule"]');
    assert.equal(send.querySelector('input[value=allow]').disabled, true);
    assert.match(zoho.textContent, /someone other than the requester approves/);
    assert.equal(zoho.querySelector('[aria-label="Delete an invoice rule"] input[value=ask]').disabled, true);
    await click(zoho.querySelector('[aria-label="Read invoices rule"] input[value=ask]'));
    await click(button('Create agent'));
    assert.equal(writes.length, 3);
    assert.ok(writes[0].path.endsWith('/agent-profiles'));
    assert.ok(writes[1].path.endsWith('/control/tools'));
    assert.ok(writes[2].path.endsWith('/control/rules'));
    assert.deepEqual(writes[1].body, { connection: 'books', enabled: true });
    assert.deepEqual(writes[2].body, { connection: 'books', operation: 'invoice.read', mode: 'ask', revision: 0 });
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chat-directory')) return response({ users, agents: [], role: 'Member' });
    if (path.endsWith('/control/available')) return response({ connections: [connection], execution_available: true });
    if (path.endsWith('/skill-writer') && !options.body) return response({ available: false, guide: '' });
    if (path.endsWith('/skill-writer/controls')) return response({ suggestions: [], by: 'rules' });
    const body = JSON.parse(options.body); writes.push({ path, body });
    return response(path.endsWith('/agent-profiles') ? { ...body, revision: 1 } : { saved: true });
  } });
});

test('step 3 adds skills from the panel under the ground rules; they are attached after create', async () => {
  const writes = [];
  const skill = { id: 's1', name: 'refund-check', description: 'Checks refund rules before promising one.', shared: true, owner_id: 'kavya', owner_name: 'Kavya Nair',
    version: 2, license: null, has_scripts: false, updated_ms: 1, files: 1, agents: [] };
  await reactFixture(async ({ render, button, click, change }) => {
    await open(render);
    await change(field('Name'), 'Iris'); await click(button('Continue')); await click(button('Continue'));
    await change(field('Ground rules'), 'Be kind.');
    assert.equal(document.querySelectorAll('.agent-create-main .agent-create-skills').length, 1, 'Skills sit under the ground rules');
    await click(button('＋ Add skills'));
    const panel = () => document.querySelector('dialog');
    assert.match(panel().textContent, /0 added to Iris/);
    await click(button('Add refund-check'));
    assert.match(panel().textContent, /1 added to Iris/);
    assert.equal(writes.length, 0, 'nothing is saved before the agent exists');
    await click(button('Done'));
    assert.equal(document.querySelectorAll('dialog').length, 0);
    assert.equal(document.activeElement?.textContent, '＋ Add skills', 'focus returns to the trigger');
    assert.match(document.querySelector('.agent-create-skills').textContent, /refund-check/);
    await click(button('Continue')); await click(button('Continue'));
    await click(button('Create agent'));
    assert.ok(writes[0].path.endsWith('/agent-profiles'));
    assert.match(writes.at(-1).path, /\/agent-profiles\/[^/]+\/skills$/);
    assert.deepEqual(writes.at(-1).body, { skill: 's1', on: true });
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chat-directory')) return response({ users, agents: [], role: 'Member' });
    if (path.endsWith('/control/available')) return response({ connections: [], execution_available: true });
    if (path.endsWith('/skills') && !options.body) return response({ workspace: [skill], mine: [], others: [], admin: false });
    const body = JSON.parse(options.body); writes.push({ path, body });
    return response(path.endsWith('/agent-profiles') ? { ...body, revision: 1 } : { skills: [skill], editable: true });
  } });
});
