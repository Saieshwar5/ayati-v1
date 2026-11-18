import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Web search (designs/web-search): built into Cuckoo, on by default, switched by
// the agent's owner or admins; the service behind it is never named.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const session = { user: { id: 'owner', email: 'owner@example.test', display_name: 'Ravi Kumar' }, workspaces: [{ id: w, name: 'Mehta Furnishings', role: 'Member' }] };
const directory = { role: 'Member', agents: [], default_model: 'fixture', system_prompt: 'Use only assigned capabilities.', providers: [], web_search: true };
const response = data => ({ ok: true, json: async () => data });
const field = label => [...document.querySelectorAll('label')].find(node => node.textContent.startsWith(label)).querySelector('input,textarea,select');
const webSwitch = () => [...document.querySelectorAll('label')].find(node => node.textContent === 'Use web search')?.querySelector('input');

async function create({ render, button, click, change }, webSearch, switchOff) {
  const context = await import('../src/workspace-context.ts');
  await context.enterWorkspace();
  const { AgentCreate, newAgent } = await import('../src/react/agents/create/create-flow.tsx');
  await render(AgentCreate, { start: newAgent(directory), directory, webSearch, close() {}, created: async () => {} });
  await change(field('Name'), 'Iris');
  await click(button('Continue'));
  if (switchOff) await click(webSwitch());
  await click(button('Continue'));
  await click(button('Customer support'));
  for (let next = 0; next < 2; next++) await click(button('Continue'));
  await click(button('Create agent'));
}
const fetcher = writes => async (path, options = {}) => {
  if (path.endsWith('/session')) return response(session);
  if (path.endsWith('/chat-directory')) return response({ users: [], agents: [], role: 'Member' });
  if (path.endsWith('/control/available')) return response({ connections: [], execution_available: true });
  const body = JSON.parse(options.body); writes.push(body);
  return response({ ...body, revision: 1 });
};

const agent = { id: 'atlas', name: 'Atlas', role: 'Orders', model: 'm', audience: 'everyone', revision: 3, settings: { avatar: 'spark', paused: false }, editable: true };
const controls = { can_manage: true, connections: [], actions: [], execution_available: true };

test('the Tools tab switch saves once, keeps the agent on after a failed save, and reports the result', async () => {
  const saves = []; let release; let fail = true; let refreshed = 0;
  await reactFixture(async ({ render, click, update }) => {
    const { ToolsTab } = await import('../src/react/agents/tools-tab.tsx');
    await render(ToolsTab, { agent, webSearch: true, saved: async () => { refreshed++; } });
    // Saved before web search existed: no setting means on.
    assert.equal(webSwitch().checked, true);
    assert.match(document.body.textContent, /Atlas can search the web and read public pages/);
    await click(webSwitch());
    assert.equal(saves.length, 1);
    assert.match(document.body.textContent, /Not saved/);
    assert.equal(webSwitch().checked, true);
    assert.equal(refreshed, 0);
    fail = false;
    await update(() => { webSwitch().click(); webSwitch().click(); });
    assert.equal(saves.length, 2, 'a second click while saving sends nothing');
    await update(() => release());
    assert.equal(saves[1].settings.web_search, false);
    assert.equal(saves[1].revision, 3);
    assert.equal(saves[1].people, undefined, 'the switch never changes who can talk to it');
    assert.equal(refreshed, 1);
    assert.match(document.body.textContent, /Atlas no longer searches the web/);
  }, { url, fetch: async (path, options = {}) => {
    if (!options.body) return response(controls);
    saves.push(JSON.parse(options.body));
    if (fail) return { ok: false, json: async () => ({ error: 'Not saved' }) };
    await new Promise(resolve => { release = resolve; });
    return response({ ...agent, revision: 4 });
  } });
});

test('people who cannot change the agent see web search but cannot switch it', async () => {
  await reactFixture(async ({ render }) => {
    const { ToolsTab } = await import('../src/react/agents/tools-tab.tsx');
    await render(ToolsTab, { agent: { ...agent, editable: false, settings: { ...agent.settings, web_search: false } }, webSearch: true, saved: async () => {} });
    assert.equal(webSwitch().disabled, true);
    assert.equal(webSwitch().checked, false);
    assert.match(document.body.textContent, /Off: Atlas answers from its apps and the Library only/);
    assert.match(document.body.textContent, /Only Atlas’s owner and admins can change this/);
  }, { url, fetch: async () => response(controls) });
});
