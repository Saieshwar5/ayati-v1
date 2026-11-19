// A new agent your assistant drafted in your chat with it (schema 139): a card
// under its reply shows the whole draft; you create the agent from it, through
// the ordinary saves, or review it in New agent. The assistant creates nothing.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=chat`;
const session = { user: { id: 'owner', email: 'owner@example.test', display_name: 'Ravi Kumar' }, workspaces: [{ id: w, name: 'Mehta Furnishings', role: 'Member' }] };
const directory = { role: 'Member', agents: [], default_model: 'fixture', system_prompt: 'x', providers: [] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const short = path => path.replace(`/api/identity/workspaces/${w}`, '');
const penny = '22222222-2222-4222-8222-222222222222';
const draft = { state: 'complete', from: 'chat', agent_id: penny, name: 'Penny', job_title: 'Invoice follow-up', created: false, replaced: false,
  reply: 'Here is a draft for Penny.', ground_rules: 'You are Penny.\nNever send a reminder without asking first.',
  skills: [{ id: '33333333-3333-4333-8333-333333333333', name: 'overdue-invoices', description: 'Use when asked which invoices are overdue.', steps: '1. List them.\n2. Give the amounts.', text: '' }],
  remove: [], reuse: [{ id: 'shared-1', name: 'complaint-reply', description: 'Use when a customer complains' }],
  missing_apps: ['Zoho Books'], notes: [], used: ['Refunds always need a person'] };

function server(state, writes = []) {
  return async (path, options = {}) => {
    const route = short(path);
    if (path.endsWith('/session')) return response(session);
    if (route === '/assistant/agent-setup/r1') return response(state.draft);
    if (route === '/agent-profiles' && !options.body) return response(directory);
    if (route === '/chat-directory') return response({ users: [{ id: 'owner', name: 'Ravi Kumar' }], agents: [], role: 'Member' });
    if (!options.body) return response({});
    const body = JSON.parse(options.body);
    writes.push({ route: route.replace(penny, ':agent'), body });
    if (state.fail && writes.length <= state.fail) return response({ error: 'Save unavailable' }, 503);
    if (route === '/agent-profiles') return response({ ...body, revision: body.revision + 1 });
    if (route === '/skills') return response({ id: body.id, name: 'overdue-invoices' });
    return response({ skills: [], editable: true });
  };
}
async function show(render, props = {}) {
  await (await import('../src/workspace-context.ts')).enterWorkspace();
  const { AgentDraftCard } = await import('../src/react/chat/agent-draft-card.tsx');
  await render(AgentDraftCard, { reply: 'r1', ready: true, assistant: 'Ayati', ...props });
}
const card = () => document.querySelector('[aria-label="Agent draft: Penny"]');

test('the card shows the whole draft; Create agent saves it once, as yours, and then says what is left', async () => {
  const writes = [], state = { draft, fail: 1 };
  await reactFixture(async ({ render, button, click, update }) => {
    await show(render);
    const text = card().textContent;
    assert.match(text, /PennyInvoice follow-upDraft · not created/);
    assert.match(text, /You are Penny\.\nNever send a reminder without asking first\./, 'every ground rule is there to read');
    assert.match(text, /overdue-invoicesUse when asked which invoices are overdue\.1\. List them\.\n2\. Give the amounts\./);
    assert.match(text, /complaint-reply.*from the library/);
    assert.match(text, /Zoho Books\. You connect them on its Connections tab after creating; Ayati can’t\./);
    assert.match(text, /What Ayati usedRefunds always need a person/);
    assert.match(text, /Everyone in Mehta Furnishings will be able to talk to it, and you will be its owner\./);
    assert.match(card().querySelector('a.ui-button').getAttribute('href'), /view=agents&setup=r1$/);
    assert.equal(writes.length, 0, 'showing the draft saves nothing');

    // A failed save keeps the draft and says why; nothing is half done.
    await update(() => { button('Create agent').click(); button('Create agent').click(); });
    assert.equal(writes.length, 1, 'one request despite a double click');
    assert.match(card().querySelector('[role=alert]').textContent, /Save unavailable/);
    assert.ok(button('Create agent'));

    await click(button('Create agent'));
    assert.deepEqual(writes.slice(1).map(write => write.route), ['/agent-profiles', '/skills', '/agent-profiles/:agent/skills', '/agent-profiles/:agent/skills']);
    const saved = writes[1].body;
    assert.equal(saved.id, penny); assert.equal(saved.name, 'Penny'); assert.equal(saved.role, 'Invoice follow-up');
    assert.equal(saved.audience, 'everyone'); assert.deepEqual(saved.people, []); assert.equal(saved.revision, 0);
    assert.equal(saved.settings.instructions, draft.ground_rules);
    assert.equal(writes[2].body.id, draft.skills[0].id, 'the skill is saved under the id that came with the draft');
    assert.equal(writes[2].body.shared, false);
    assert.equal(writes[2].body.files[0].text, '---\nname: overdue-invoices\ndescription: Use when asked which invoices are overdue.\n---\n1. List them.\n2. Give the amounts.\n');
    assert.deepEqual(writes[3].body, { skill: draft.skills[0].id, on: true });
    assert.deepEqual(writes[4].body, { skill: 'shared-1', on: true });
    assert.match(card().textContent, /Created.*Penny is created and you are its owner\. It can’t use Zoho Books until you connect it\./s);
    assert.equal(button('Create agent'), undefined);
    assert.match(card().querySelector('a.ui-button.primary').getAttribute('href'), new RegExp(`selected=${penny}&section=connections`));
  }, { url, fetch: server(state, writes) });
});

test('nothing shows while the reply is written; an earlier draft gives way; one without a name can’t be created', async () => {
  const state = { draft: { state: 'streaming' } };
  const shown = () => Boolean(document.querySelector('.agent-draft-card'));
  await reactFixture(async ({ render }) => {
    await show(render, { ready: false });
    assert.equal(shown(), false);
    state.draft = draft;
    await show(render, { ready: true });
    assert.equal(shown(), true, 'read again once the reply is complete');
  }, { url, fetch: server(state) });
  await reactFixture(async ({ render, button }) => {
    await show(render);
    assert.equal(shown(), false);
    assert.match(document.querySelector('.agent-draft-replaced').textContent, /Penny · earlier draft, replaced by the one below/);
    assert.equal(button('Create agent'), undefined);
  }, { url, fetch: server({ draft: { ...draft, replaced: true } }) });
  // Replaced after the card was read: Create saves nothing and the card gives way.
  const late = { draft }, writes = [];
  await reactFixture(async ({ render, button, click }) => {
    await show(render);
    late.draft = { ...draft, replaced: true };
    await click(button('Create agent'));
    assert.equal(writes.length, 0);
    assert.equal(shown(), false);
    assert.match(document.querySelector('.agent-draft-replaced').textContent, /replaced by the one below/);
  }, { url, fetch: server(late, writes) });
  await reactFixture(async ({ render, button }) => {
    await show(render);
    assert.equal(button('Create agent').disabled, true);
    assert.match(document.querySelector('.agent-draft-card').textContent, /It has no name yet\. Ask Ayati to name it, or review it in New agent\./);
  }, { url, fetch: server({ draft: { ...draft, name: null } }) });
  await reactFixture(async ({ render, button }) => {
    await show(render);
    assert.match(card().textContent, /Created/);
    assert.equal(button('Create agent'), undefined);
  }, { url, fetch: server({ draft: { ...draft, created: true } }) });
});

test('Review in New agent opens the draft on the Agents page, where the steps keep it', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click }) => {
    await (await import('../src/workspace-context.ts')).enterWorkspace();
    const { AgentsPage } = await import('../src/react/agents/directory.tsx');
    await render(AgentsPage);
    const field = label => [...document.querySelectorAll('label')].find(node => node.textContent.startsWith(label)).querySelector('input,textarea,select');
    assert.equal(field('Ground rules').value, draft.ground_rules, 'it opens at What it does, filled in');
    assert.match(document.querySelector('.draft-skills').textContent, /overdue-invoices/);
    assert.match(document.querySelector('.skill-chips').textContent, /complaint-reply/);
    assert.equal(writes.length, 0, 'opening it saves nothing');
    await click(button('Back'));
    assert.equal(field('Name').value, 'Penny'); assert.equal(field('Job title').value, 'Invoice follow-up');
    assert.deepEqual(writes.map(write => write.route), ['/agent-draft']);
    assert.equal(writes[0].body.id, penny);
  }, { url: `http://localhost/w/${w}/?view=agents&setup=r1`, fetch: server({ draft }, writes) });
});
