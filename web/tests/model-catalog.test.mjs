// MG1: with Cuckoo's model catalog, people choose models by Cuckoo's names and never
// a provider; the agent's Environment tab and Settings → Workspace both use the catalog.
import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const model = (id, name, extra = {}) => ({ id, name, family: '', description: '', tags: [], context: 0, supports: { tools: true, vision: false },
  price: { input: 0.9, output: 0.9 }, status: 'available', replacement: null, ...extra });
const catalog = { catalog_on: true, models: [
  model('llama-70b', 'Llama 70B', { tags: ['fast'], description: 'Quick everyday answers.', context: 128000 }),
  model('reasoner', 'Reasoner', { price: { input: 3, output: 8 }, supports: { tools: true, vision: true } }),
  model('old', 'Old model', { status: 'retiring', replacement: 'llama-70b' }),
] };
const settings = { provider: '', instructions: '', escalation: '', avatar: 'spark', memory: false, browser: false, history: 'all', paused: false };
const session = { user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };

test('An agent on a retiring model is told to move; the picker shows catalog models only and saves no provider, once', async () => {
  const agent = { id: 'alpha', name: 'Meera', role: '', model: 'old', audience: 'selected', revision: 1, settings, editable: true };
  const directory = { role: 'Admin', agents: [agent], default_model: 'llama-70b', providers: [{ id: 'openai', name: 'OpenAI', configured: true }], system_prompt: '', catalog };
  const writes = []; let complete;
  await reactFixture(async ({ render, button, click, update }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { EnvironmentTab } = await import('../src/react/agents/environment-tab.tsx');
    let refreshed = 0;
    await render(EnvironmentTab, { agent, directory, saved: async () => { refreshed += 1; } });
    const text = document.body.textContent;
    assert.match(text, /Old model is being retired\. Choose Llama 70B to keep Meera working/);
    assert.match(text, /Llama 70B.*Workspace default.*fast.*Quick everyday answers/);
    assert.match(text, /Uses tools · Reads images · \$3 in · \$8 out per million tokens/);
    assert.equal(text.includes('OpenAI'), false, 'no provider is shown');
    assert.equal(document.querySelector('input[value=old]').disabled, false, 'the current model stays selected');
    assert.equal(button('Save model').disabled, true, 'a retiring model cannot be saved again');
    await click(document.querySelector('input[value=reasoner]'));
    await update(() => { button('Save model').click(); button('Save model').click(); });
    assert.equal(writes.length, 1, 'one save for a double click');
    await update(() => complete(response({ error: 'Save failed' }, 503)));
    assert.match(document.body.textContent, /Save failed/);
    assert.equal(document.querySelector('input[value=reasoner]').checked, true, 'the choice stays after a failure');
    await click(button('Save model'));
    await update(() => complete(response({ ...agent, model: 'reasoner', revision: 2 })));
    assert.equal(writes.length, 2);
    assert.deepEqual([writes[1].model, writes[1].settings.provider], ['reasoner', '']);
    assert.match(document.body.textContent, /Model saved/);
    assert.equal(refreshed, 1);
  }, { url: `http://localhost/w/${w}/?view=agents`, fetch: async (path, options = {}) => {
    if (options.method === 'POST') { writes.push(JSON.parse(options.body)); return new Promise(resolve => { complete = resolve; }); }
    return response(session);
  } });
});

test('Admins limit the workspace to catalog models without naming providers', async () => {
  const saved = { name: 'Store', timezone: 'Asia/Kolkata', colour: 'teal', approval_backup_hours: 4,
    models: [{ provider: '', model: 'llama-70b' }, { provider: 'openai', model: 'gpt-4o' }], default_model: 'llama-70b', delete_at_ms: null, delete_requested_by: null };
  const writes = [];
  await reactFixture(async ({ render, button, click, change }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { SettingsPage } = await import('../src/react/settings/settings-page.tsx');
    await render(SettingsPage, { href: `http://localhost/w/${w}/?view=settings&section=workspace` });
    const text = document.body.textContent;
    assert.match(text, /Llama 70B/);
    assert.match(text, /gpt-4o.*Not one of Cuckoo’s models/);
    assert.equal(document.querySelector('[aria-label="Provider"]'), null, 'no provider to choose');
    const options = [...document.querySelector('[aria-label="Model to add"]').options].map(o => o.textContent);
    assert.deepEqual(options, ['Choose a model…', 'Reasoner'], 'listed and retiring models are not offered again');
    await change(document.querySelector('[aria-label="Model to add"]'), 'reasoner');
    await click(button('Add model'));
    await click(button('Remove gpt-4o'));
    await click(document.querySelector('.settings-rules button[type=submit]'));
    assert.deepEqual(writes.at(-1).models, [{ provider: '', model: 'llama-70b' }, { provider: '', model: 'reasoner' }]);
    assert.match(document.body.textContent, /Workspace settings saved/);
  }, { url: `http://localhost/w/${w}/?view=settings&section=workspace`, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/models')) return response(catalog);
    if (options.body) { writes.push(JSON.parse(options.body)); return response({ ...saved, ...JSON.parse(options.body) }); }
    return response(saved);
  } });
});
