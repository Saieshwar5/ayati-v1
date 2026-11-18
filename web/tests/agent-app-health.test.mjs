import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Health tags where agents get their apps, and getting another app from there
// (designs/connector-screens 5 and 6).
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const session = role => response({ user: { id: 'owner', email: 'owner@example.test', display_name: 'Ravi Kumar' }, workspaces: [{ id: w, name: 'Mehta Furnishings', role }] });
const directory = { role: 'Member', agents: [], default_model: 'fixture', system_prompt: 'Use only assigned capabilities.', providers: [] };
const operation = (id, title, effect, meaning, mode) => ({ id, title, effect, meaning, workspace_mode: mode, needs_other: false, agent_mode: null, mode, revision: 0 });
const connection = (id, name, state, on = false) => ({ id, server: id, name, account: `${id}@mehta.test`, revision: 1, on, ...(state ? { state } : {}),
  operations: [operation(`${id}.read`, 'Find emails', 'read', 'read', 'allow'), operation(`${id}.send`, 'Send an email', 'write', 'sends_outside', 'ask')] });
const connections = [connection('gmail', 'Gmail', 'writes_paused', true), connection('zoho', 'Zoho Books', 'reconnect'), connection('slack', 'Slack', 'paused'), connection('calendar', 'Google Calendar')];
const field = label => [...document.querySelectorAll('label')].find(node => node.textContent.startsWith(label)).querySelector('input,textarea,select');
const tags = () => Object.fromEntries([...document.querySelectorAll('.agent-app-name')].map(row => [row.querySelector('strong').textContent, row.querySelector('.health-tag')?.textContent ?? '']));
async function setup(render, change, click, button) {
  const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
  const { AgentCreate, newAgent } = await import('../src/react/agents/create/create-flow.tsx');
  await render(AgentCreate, { start: newAgent(directory), directory, close() {}, created: async () => {} });
  await change(field('Name'), 'Iris');
  await click(button('Continue'));
}

const agent = { id: 'atlas', name: 'Atlas', role: 'Orders', model: 'm', audience: 'everyone', revision: 1, settings: {}, editable: true };
const controls = extra => ({ can_manage: true, can_manage_connections: true, viewer: 'owner', execution_available: true, connections, actions: [
  { id: 'u1', connection: 'gmail', operation: 'gmail.send', title: 'Send an email', effect: 'write', arguments: {}, state: 'uncertain', result: null, chat: 'c', thread: null, job: 'j', approver: 'owner', created: 1, updated: 1 }], ...extra });

test('The Tools tab shows the same tags with the fix; Check it opens the outcome dialog', async () => {
  const reads = [];
  await reactFixture(async ({ render, click, button }) => {
    const { ToolsTab } = await import('../src/react/agents/tools-tab.tsx');
    await render(ToolsTab, { agent });
    assert.deepEqual(tags(), { Gmail: 'Writes paused', 'Zoho Books': 'Needs reconnecting', Slack: 'Paused by Cuckoo', 'Google Calendar': '' });
    const gmail = document.querySelector('[aria-label="Gmail"]');
    assert.match(gmail.textContent, /Atlas can read\. Writing waits until someone checks one earlier action\. Check it/);
    assert.match(document.querySelector('[aria-label="Zoho Books"]').textContent, /The sign-in expired\. Atlas can’t use Zoho Books until an admin reconnects it\. Reconnect on Apps/);
    assert.match(document.querySelector('[aria-label="Zoho Books"] a').href, /view=apps&manage=1/);
    assert.match(document.querySelector('[aria-label="Slack"]').textContent, /Cuckoo paused Slack for a short while/);
    await click(button('Check it'));
    assert.ok(reads.some(path => path.endsWith('/control/actions/u1')));
    assert.equal(document.querySelector('dialog[open] h2').textContent, 'Did this go out?');
  }, { url, fetch: async path => {
    reads.push(path);
    if (path.includes('/control/actions/')) return response({ id: 'u1', title: 'Send an email', state: 'uncertain', arguments: {}, agent_name: 'Atlas', account_name: 'Gmail', settle: { can_settle: true, checks: [] } });
    return response(controls());
  } });
});

test('With no apps the Tools tab points to Apps through a request, not to Settings', async () => {
  await reactFixture(async ({ render, button }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { ToolsTab } = await import('../src/react/agents/tools-tab.tsx');
    await render(ToolsTab, { agent });
    assert.match(document.body.textContent, /No apps yet\. Once an admin connects one, it can be switched on here\./);
    assert.doesNotMatch(document.body.textContent, /Settings → Connections/);
    assert.ok(button('Ask an admin'));
  }, { url, fetch: async path => path.endsWith('/session') ? session('Member') : response(controls({ connections: [], actions: [], can_manage_connections: false })) });
});
