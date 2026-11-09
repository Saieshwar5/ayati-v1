// The model on the last create step: the workspace default unless changed there,
// chosen only from the models the admin allows, with a warning when the chosen
// model can't use the tools picked earlier.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const session = { user: { id: 'owner', email: 'owner@example.test', display_name: 'Ravi Kumar' }, workspaces: [{ id: w, name: 'Store', role: 'Member' }] };
const response = data => ({ ok: true, json: async () => data });
const model = (id, name, extra = {}) => ({ id, name, family: '', description: '', tags: [], context: 0, supports: { tools: true, vision: false },
  price: { input: 1, output: 2 }, status: 'available', replacement: null, ...extra });
const catalog = { catalog_on: true, models: [
  model('fast', 'Fast model', { description: 'Quick everyday answers.' }),
  model('careful', 'Careful model'),
  model('writer', 'Writer', { supports: { tools: false, vision: false } }),
  model('old', 'Old model', { status: 'retiring', replacement: 'fast' }),
  model('hidden', 'Not allowed'),
] };
const allowed = ['fast', 'careful', 'writer', 'old'].map(id => ({ provider: '', model: id }));
const base = { role: 'Member', agents: [], default_model: 'fast', system_prompt: '', providers: [{ id: 'openai', name: 'OpenAI', configured: true }] };
const connection = { id: 'books', server: 'books', name: 'Zoho Books', account: 'Accounts', revision: 1, on: false, operations: [
  { id: 'invoice.read', title: 'Read invoices', effect: 'read', meaning: 'read', workspace_mode: 'allow', needs_other: false, agent_mode: null, mode: 'allow', revision: 0 }] };

/** Opens create, names the agent and walks to the last step; `app` switches Zoho Books on. */
async function toLastStep(helpers, directory, app = false) {
  const { render, button, click, change } = helpers;
  const context = await import('../src/workspace-context.ts');
  await context.enterWorkspace();
  const { AgentCreate, newAgent } = await import('../src/react/agents/create/create-flow.tsx');
  await render(AgentCreate, { start: newAgent(directory), directory, close() {}, created: async () => {} });
  const field = label => [...document.querySelectorAll('label')].find(node => node.textContent.startsWith(label)).querySelector('input,textarea');
  await change(field('Name'), 'Ledger'); await click(button('Continue'));
  if (app) await click(document.querySelector('.agent-apps li [role=switch]'));
  await click(button('Continue'));
  await change(field('Ground rules'), 'Keep the books tidy.');
  await click(button('Continue')); await click(button('Continue'));
}
const box = () => document.querySelector('.agent-create-model');
const choices = () => [...box().querySelectorAll('input[type=radio]')];
const fetchFor = (writes, connections = []) => async (path, options = {}) => {
  if (path.endsWith('/session')) return response(session);
  if (path.endsWith('/chat-directory')) return response({ users: [{ id: 'owner', name: 'Ravi Kumar' }], agents: [], role: 'Member' });
  if (path.endsWith('/control/available')) return response({ connections, execution_available: true });
  if (path.endsWith('/skill-writer') && !options.body) return response({ available: false, guide: '' });
  if (path.endsWith('/skill-writer/controls')) return response({ suggestions: [], by: 'rules' });
  const body = JSON.parse(options.body); writes.push({ path, body });
  return response(path.endsWith('/agent-profiles') ? { ...body, revision: 1 } : { saved: true });
};

test('the last step starts on the workspace default; Change offers only allowed, current models and saves no provider', async () => {
  const writes = [];
  await reactFixture(async helpers => {
    const { button, click } = helpers;
    await toLastStep(helpers, { ...base, catalog, models: allowed });
    assert.match(box().textContent, /Model · Fast modelWorkspace default/);
    assert.equal(choices().length, 0, 'collapsed until Change');
    await click(button('Change'));
    assert.equal(button('Done').getAttribute('aria-expanded'), 'true');
    assert.deepEqual(choices().map(input => input.value), ['fast', 'careful', 'writer'], 'retiring and unlisted models are not offered');
    assert.equal(choices()[0].checked, true);
    await click(choices()[1]);
    assert.match(box().textContent, /Model · Careful model/);
    assert.doesNotMatch(box().querySelector('.agent-create-model-line').textContent, /Workspace default/);
    await click(button('Create agent'));
    assert.equal(writes.length, 1);
    assert.equal(writes[0].body.model, 'careful');
    assert.equal(writes[0].body.settings.provider, '');
  }, { url, fetch: fetchFor(writes) });
});

test('a model that can’t use tools is flagged only when tools were chosen, with a one-click fix', async () => {
  const writes = [];
  await reactFixture(async helpers => {
    const { button, click } = helpers;
    await toLastStep(helpers, { ...base, catalog, models: allowed }, true);
    await click(button('Change'));
    await click(choices().find(input => input.value === 'writer'));
    assert.match(box().textContent, /Writer can’t use tools, so Ledger won’t be able to use the apps or tools you chose/);
    await click(button('Use Fast model instead'));
    assert.ok(!box().querySelector('.agent-create-model-warning'));
    await click(button('Create agent'));
    assert.equal(writes[0].body.model, 'fast');
  }, { url, fetch: fetchFor(writes, [connection]) });
  await reactFixture(async helpers => {
    const { button, click } = helpers;
    await toLastStep(helpers, { ...base, catalog, models: allowed });
    await click(button('Change'));
    await click(choices().find(input => input.value === 'writer'));
    assert.ok(!box().querySelector('.agent-create-model-warning'), 'no tools chosen, nothing to warn about');
  }, { url, fetch: fetchFor([]) });
});

test('without a catalog the admin’s provider and model pairs are offered and the provider is saved', async () => {
  const writes = [];
  const models = [{ provider: 'fireworks-ai', model: 'fast' }, { provider: 'openai', model: 'gpt-5-mini' }];
  await reactFixture(async helpers => {
    const { button, click } = helpers;
    await toLastStep(helpers, { ...base, models, providers: [...base.providers, { id: 'fireworks-ai', name: 'Fireworks', configured: true }] });
    assert.match(box().textContent, /Model · fastWorkspace defaultFireworks/);
    await click(button('Change'));
    assert.deepEqual(choices().map(input => input.value), ['fireworks-ai/fast', 'openai/gpt-5-mini']);
    await click(choices()[1]);
    await click(button('Create agent'));
    assert.equal(writes[0].body.model, 'gpt-5-mini');
    assert.equal(writes[0].body.settings.provider, 'openai');
  }, { url, fetch: fetchFor(writes) });
});

test('with only one allowed model there is nothing to change', async () => {
  await reactFixture(async helpers => {
    await toLastStep(helpers, { ...base, catalog, models: [{ provider: '', model: 'fast' }] });
    assert.match(box().textContent, /Model · Fast model/);
    assert.equal([...box().querySelectorAll('button')].length, 0);
  }, { url, fetch: fetchFor([]) });
});
