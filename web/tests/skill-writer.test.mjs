// Skills written in chat (designs/agent-built-skills 2 and 5): the writer only
// drafts; drafts are saved when the agent is created, and changes to a saved
// agent are applied one by one.
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
const response = data => ({ ok: true, json: async () => data });
const books = { id: 'books', server: 'books', name: 'Zoho Books', revision: 1, on: false, operations: [
  { id: 'order.read', title: 'Read order', effect: 'read', meaning: 'read', workspace_mode: 'allow', needs_other: false, agent_mode: null, mode: 'allow', revision: 0 }] };
const field = label => [...document.querySelectorAll('label')].find(node => node.textContent.startsWith(label)).querySelector('input,textarea,select');
const composer = () => document.querySelector('#setup-chat-input');

async function openCreate(render) {
  const context = await import('../src/workspace-context.ts');
  await context.enterWorkspace();
  const { AgentCreate, newAgent } = await import('../src/react/agents/create/create-flow.tsx');
  const created = [];
  await render(AgentCreate, { start: newAgent(directory), directory, close() {}, created: async id => { created.push(id); } });
  return created;
}

test('the setup chat writes ground rules and skills, and they are saved only when the agent is created', async () => {
  const asked = [], writes = [];
  const answers = [
    { reply: 'I wrote your ground rules and one skill.', ground_rules: 'You are Iris.\nNever give discounts.',
      skills: [{ name: 'refund-check', description: 'Use when a customer asks for a refund', steps: '1. Read the order.\n2. Ask Meera above 5000.', text: '' }],
      remove: [], reuse: [], missing_apps: ['Shiprocket'], notes: [] },
    { reply: 'Changed the limit and added the workspace skill.', ground_rules: null,
      skills: [{ name: 'refund-check', description: 'Use when a customer asks for a refund', steps: '1. Read the order.\n2. Ask Meera above 3000.', text: '' }],
      remove: [], reuse: [{ id: 'shared-1', name: 'complaint-reply', description: 'Use when a customer complains' }], missing_apps: [], notes: [] },
  ];
  await reactFixture(async ({ render, button, click, change, update }) => {
    const created = await openCreate(render);
    await change(field('Name'), 'Iris'); await click(button('Continue'));
    await click(document.querySelector('[role=switch]'));
    await click(button('Continue'));
    assert.ok(composer(), 'the chat is the main way in');
    await click(button('Customer support'));
    assert.match(composer().value, /You are Iris, our customer support assistant/);
    await change(composer(), 'Answer customers about orders. Never give discounts. Above ₹5,000 ask Meera.');
    // A double click sends one turn.
    await update(() => { button('Send').click(); button('Send').click(); });
    assert.equal(asked.length, 1);
    assert.deepEqual(asked[0], { name: 'Iris', role: '', connections: ['books'], ground_rules: '', skills: [],
      messages: [{ role: 'person', text: 'Answer customers about orders. Never give discounts. Above ₹5,000 ask Meera.' }] });
    assert.match(document.querySelector('.setup-chat-messages').textContent, /I wrote your ground rules and one skill\./);
    assert.equal(field('Ground rules').value, 'You are Iris.\nNever give discounts.');
    assert.match(document.querySelector('.draft-skills').textContent, /refund-check NewUse when a customer asks for a refund2 steps/);
    assert.match(document.querySelector('.setup-chat-notes').textContent, /Shiprocket isn’t one of the apps here/);
    assert.equal(composer().value, '', 'a sent message leaves the composer');
    assert.equal(writes.length, 0, 'nothing is saved yet');

    await click(button('Ask Iris about refund-check'));
    assert.equal(composer().value, 'About refund-check: ');
    await change(composer(), 'About refund-check: the limit is now ₹3,000.');
    await click(button('Send'));
    assert.equal(asked[1].focus, 'refund-check');
    assert.deepEqual(asked[1].skills, [{ name: 'refund-check', description: 'Use when a customer asks for a refund', steps: '1. Read the order.\n2. Ask Meera above 5000.' }]);
    assert.equal(asked[1].messages.length, 3);
    assert.match(document.querySelector('.agent-create-skills').textContent, /complaint-reply/);

    await click(button('Edit refund-check'));
    await change(field('Steps'), '1. Read the order.\n2. Ask Meera above 3000.\n3. Say refunds take a week.');
    await click(button('Save draft'));
    assert.match(document.querySelector('.draft-skills').textContent, /3 steps/);

    await click(button('Continue')); await click(button('Continue'));
    await click(button('Create agent'));
    const paths = writes.map(write => write.path.replace(/.*\/workspaces\/[^/]+/, ''));
    assert.deepEqual(paths.map(path => path.replace(/agent-profiles\/[^/]+/, 'agent-profiles/:agent')),
      ['/agent-profiles', '/agent-profiles/:agent/control/tools', '/skills', '/agent-profiles/:agent/skills', '/agent-profiles/:agent/skills']);
    const skill = writes[2].body;
    assert.equal(skill.shared, false);
    assert.equal(skill.files[0].text, '---\nname: refund-check\ndescription: Use when a customer asks for a refund\n---\n1. Read the order.\n2. Ask Meera above 3000.\n3. Say refunds take a week.\n');
    assert.deepEqual(writes[3].body, { skill: skill.id, on: true });
    assert.deepEqual(writes[4].body, { skill: 'shared-1', on: true });
    assert.equal(writes[0].body.settings.instructions, 'You are Iris.\nNever give discounts.');
    assert.equal(created.length, 1);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chat-directory')) return response({ users, agents: [], role: 'Member' });
    if (path.endsWith('/control/available')) return response({ connections: [books], execution_available: true });
    if (path.endsWith('/skill-writer') && !options.body) return response({ available: true, guide: 'GUIDE' });
    if (path.endsWith('/skill-writer/controls')) return response({ suggestions: [], by: 'rules' });
    const body = JSON.parse(options.body);
    if (path.endsWith('/skill-writer')) { asked.push(body); return response(answers[asked.length - 1]); }
    writes.push({ path, body });
    if (path.endsWith('/agent-profiles')) return response({ ...body, revision: 1 });
    if (path.endsWith('/skills') && body.files) return response({ ...body, id: body.id, name: 'refund-check' });
    return response({ skills: [], editable: true, saved: true });
  } });
});

test('a failed turn keeps what was typed and says why; without a writer the step is written by hand', async () => {
  let available = true;
  await reactFixture(async ({ render, button, click, change }) => {
    await openCreate(render);
    await change(field('Name'), 'Iris'); await click(button('Continue')); await click(button('Continue'));
    await change(composer(), 'Answer order questions.');
    await click(button('Send'));
    assert.equal(composer().value, 'Answer order questions.');
    assert.match(document.querySelector('.setup-chat [role=alert]').textContent, /The skill writer couldn’t answer/);
    assert.equal(document.querySelectorAll('.setup-chat-message.person').length, 0);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chat-directory')) return response({ users, agents: [], role: 'Member' });
    if (path.endsWith('/control/available')) return response({ connections: [], execution_available: true });
    if (path.endsWith('/skill-writer') && !options.body) return response({ available, guide: '' });
    return { ok: false, status: 502, json: async () => ({ error: 'The skill writer couldn’t answer. Try again, or write the skill yourself.' }) };
  } });
  available = false;
  await reactFixture(async ({ render, button, click, change }) => {
    await openCreate(render);
    await change(field('Name'), 'Iris'); await click(button('Continue')); await click(button('Continue'));
    assert.equal(composer(), null);
    assert.match(document.body.textContent, /Writing skills in chat isn’t set up here/);
    await click(button('Write a skill'));
    await change(field('Name'), 'Refund Check'); await change(field('Use it when'), 'Use when a refund is asked');
    await change(field('Steps'), '1. Check the order.');
    await click(button('Save draft'));
    assert.match(document.querySelector('.draft-skills').textContent, /refund-check New/);
    await click(button('Write a skill'));
    await change(field('Name'), 'refund check'); await change(field('Use it when'), 'Use when'); await change(field('Steps'), '1.');
    await click(button('Save draft'));
    assert.match(document.querySelector('dialog').textContent, /Iris already has a skill called refund-check/);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chat-directory')) return response({ users, agents: [], role: 'Member' });
    if (path.endsWith('/control/available')) return response({ connections: [], execution_available: true });
    if (path.endsWith('/skill-writer') && !options.body) return response({ available, guide: '' });
    return response({});
  } });
});

test('on the agent page, a proposed change shows as a diff and saves as the next version with its other files', async () => {
  const settings = { provider: '', instructions: 'You are Atlas.', escalation: '', avatar: 'robot-03', memory: false, browser: false, history: 'all', paused: false };
  const agent = { id: 'atlas', name: 'Atlas', role: 'Orders assistant', model: 'fixture', audience: 'everyone', revision: 5, settings, editable: true, owner_id: 'owner', people: [], managers: ['owner'] };
  const skill = { id: 's1', name: 'refund-check', description: 'Use when a refund is asked', shared: false, owner_id: 'owner', owner_name: 'Ravi Kumar', version: 2, license: null, has_scripts: false, updated_ms: 1, files: 2, agents: ['atlas'] };
  const files = [{ path: 'SKILL.md', text: '---\nname: refund-check\ndescription: Use when a refund is asked\n---\n1. Ask Meera above 5000.\n' }, { path: 'references/policy.md', text: 'Returns within 7 days.' }];
  const asked = [], versions = [];
  await reactFixture(async ({ render, button, click, change }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { AgentWorkspace } = await import('../src/react/agents/workspace.tsx');
    await render(AgentWorkspace, { agent, directory: { ...directory, agents: [agent] }, revision: 0, back() {}, guide() {}, control() {}, openChat() {}, busy: false,
      section: 'Instructions', selectSection() {}, saved: async () => {} });
    const chat = document.querySelector('[aria-label="Change with Atlas"]');
    assert.ok(chat, 'the owner sees the chat');
    await change(composer(), 'Refunds above ₹3,000 now go to Meera.');
    await click(button('Send'));
    assert.deepEqual(asked[0].skills, [{ name: 'refund-check', description: 'Use when a refund is asked', steps: '1. Ask Meera above 5000.', saved: true }]);
    assert.equal(asked[0].agent, 'atlas');
    const proposals = document.querySelector('[aria-label="Proposed changes"]').textContent;
    assert.match(proposals, /Ground rules/);
    assert.match(proposals, /refund-check · version 2/);
    assert.match(proposals, /− 1\. Ask Meera above 5000\.\n\+ 1\. Ask Meera above 3000\./);
    await click(button('Use in the instructions box'));
    assert.equal(field('Ground rules').value, 'You are Atlas.\nRefunds above ₹3,000 go to Meera.');
    assert.equal(button('Save changes').disabled, false, 'saved only with Save changes');
    await click(button('Save as version 3'));
    assert.equal(versions.length, 1);
    assert.equal(versions[0].version, 2);
    assert.deepEqual(versions[0].files, [files[1], { path: 'SKILL.md', text: '---\nname: refund-check\ndescription: Use when a refund is asked\n---\n1. Ask Meera above 3000.\n' }]);
    assert.match(document.querySelector('[aria-label="Proposed changes"]').textContent, /Saved as version 3\./);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/skill-writer') && !options.body) return response({ available: true, guide: '' });
    if (path.endsWith('/skill-writer')) {
      asked.push(JSON.parse(options.body));
      return response({ reply: 'Here is the change.', ground_rules: 'You are Atlas.\nRefunds above ₹3,000 go to Meera.',
        skills: [{ name: 'refund-check', description: 'Use when a refund is asked', steps: '1. Ask Meera above 3000.', text: '' }], remove: [], reuse: [], missing_apps: [], notes: [] });
    }
    if (path.endsWith('/skills/s1/versions')) { versions.push(JSON.parse(options.body)); return response({ ...skill, version: 3, files }); }
    if (path.endsWith('/skills/s1')) return response({ ...skill, files, hash: 'h', shown_version: 2, versions: [] });
    if (path.endsWith('/agent-profiles/atlas/skills')) return response({ skills: [skill], editable: true });
    if (path.endsWith('/knowledge')) return response({ notes: [] });
    if (path.includes('/library-always/')) return response({ items: [], given: [], hidden: 0, chosen: 0, editable: false, most: 5, budget: 8000 });
    if (path.endsWith('/chat-directory')) return response({ users, agents: [], role: 'Member' });
    return response({});
  } });
});

test('Controls arrive suggested with reasons; a person’s own choice survives new suggestions and Reset brings them back', async () => {
  const asked = [], writes = [];
  const ops = [
    { id: 'order.read', title: 'Read order', effect: 'read', meaning: 'read', workspace_mode: 'allow', needs_other: false, agent_mode: null, mode: 'allow', revision: 0 },
    { id: 'discount.apply', title: 'Apply a discount', effect: 'write', meaning: 'money', workspace_mode: 'allow', needs_other: false, agent_mode: null, mode: 'allow', revision: 0 }];
  const zoho = { ...books, operations: ops };
  const suggestions = [{ connection: 'books', operation: 'order.read', mode: 'allow', reason: 'refund-check uses it' },
    { connection: 'books', operation: 'discount.apply', mode: 'block', reason: 'You said: “Never give discounts.”' }];
  await reactFixture(async ({ render, button, click, change }) => {
    await openCreate(render);
    await change(field('Name'), 'Iris'); await click(button('Continue'));
    await click(document.querySelector('[role=switch]')); await click(button('Continue'));
    await change(field('Ground rules'), 'Never give discounts.');
    await click(button('Continue')); await click(button('Continue'));
    const books = () => document.querySelector('[aria-label="Zoho Books"]');
    // Strict Mode starts the effect twice; the first read is aborted. Every read is for this setup.
    const first = asked.length;
    assert.ok(first >= 1);
    for (const body of asked) assert.deepEqual(body, { name: 'Iris', role: '', audience: 'everyone', ground_rules: 'Never give discounts.', skills: [], connections: ['books'] });
    assert.match(document.querySelector('.agent-tools-note').textContent, /Iris suggested these from its apps, skills, what you said/);
    assert.equal(books().querySelector('[aria-label="Apply a discount rule"] input[value=block]').checked, true);
    assert.match(books().textContent, /Suggested Block: You said: “Never give discounts\.”/);
    // The person loosens reading; going back and forth keeps it and asks again only for a new setup.
    await click(books().querySelector('[aria-label="Read order rule"] input[value=ask]'));
    await click(button('Back')); await click(button('Continue'));
    assert.equal(asked.length, first, 'the same setup is not asked again');
    assert.equal(books().querySelector('[aria-label="Read order rule"] input[value=ask]').checked, true);
    await click(button('Reset to suggestions'));
    assert.equal(books().querySelector('[aria-label="Read order rule"] input[value=allow]').checked, true);
    await click(button('Create agent'));
    const rules = writes.filter(write => write.path.endsWith('/control/rules')).map(write => write.body);
    assert.deepEqual(rules, [{ connection: 'books', operation: 'discount.apply', mode: 'block', revision: 0 }], 'only what is stricter than the admin is saved');
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chat-directory')) return response({ users, agents: [], role: 'Member' });
    if (path.endsWith('/control/available')) return response({ connections: [zoho], execution_available: true });
    if (path.endsWith('/skill-writer') && !options.body) return response({ available: false, guide: '' });
    const body = JSON.parse(options.body);
    if (path.endsWith('/skill-writer/controls')) { asked.push(body); return response({ suggestions, by: 'writer' }); }
    writes.push({ path, body });
    return response(path.endsWith('/agent-profiles') ? { ...body, revision: 1 } : { saved: true });
  } });
});
