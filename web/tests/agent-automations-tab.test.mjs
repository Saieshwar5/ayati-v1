// The agent's Automations tab: work at a time is asked for in Chat and becomes a
// task; triggers are set up here, with triggers set up from an app listed beside
// the agent's own.
import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const session = { user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };
const agent = { id: 'agent', name: 'Nova', role: 'Help customers', model: 'fixture', audience: 'everyone', revision: 3, editable: true, settings: { instructions: 'Help customers with orders.' } };
const response = data => ({ ok: true, status: 200, json: async () => data });

async function openPane(render) {
  const context = await import('../src/workspace-context.ts');
  await context.enterWorkspace();
  const { AutomationPane } = await import('../src/react/agents/automation/pane.tsx');
  await render(AutomationPane, { agent });
}

test('work at a time points to Chat and the Calendar; triggers sit beside it', async () => {
  const reads = [];
  await reactFixture(async ({ render, update }) => {
    await openPane(render);
    await update(async () => { await new Promise(resolve => setTimeout(resolve, 20)); });
    const groups = [...document.querySelectorAll('.automation-group h4')].map(node => node.textContent);
    assert.deepEqual(groups.slice(0, 1), ['At a time']);
    assert.ok(groups.includes('When something happens'));
    assert.match(document.body.textContent, /Ask Nova in Chat, in your own words\. It becomes a task/);
    assert.equal([...document.querySelectorAll('a.ui-button')].find(a => a.textContent === 'Open the Calendar').getAttribute('href'), `/w/${w}/?view=activity&tab=planned`);
    assert.match(document.body.textContent, /No triggers yet\./);
  }, { url, fetch: async path => {
    reads.push(path);
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/event-triggers')) return response({ triggers: [], choices: [], apps: [], execution_available: true });
    return response({});
  } });
});

test('old and linked tab names open Automations', async () => {
  const { sectionNamed, agentSectionUrl } = await import('../src/react/agents/workspace-tabs.tsx');
  assert.equal(sectionNamed('Automations'), 'Automations');
  assert.equal(sectionNamed('automations'), 'Automations');
  assert.equal(sectionNamed('tools'), 'Connections', 'a link to the old Tools tab opens Connections');
  assert.equal(sectionNamed('environment'), 'Settings');
  assert.equal(sectionNamed('activity'), 'Overview');
  assert.equal(sectionNamed('nonsense'), undefined);
  assert.match(agentSectionUrl('a b', 'Automations'), /\?view=agents&selected=a%20b&section=automations$/);
});

test('people who only talk to the agent are pointed to Chat and the Calendar', async () => {
  let opened = 0;
  await reactFixture(async ({ render, button, click }) => {
    const context = await import('../src/workspace-context.ts');
    await context.enterWorkspace();
    const { AutomationPane } = await import('../src/react/agents/automation/pane.tsx');
    await render(AutomationPane, { agent: { ...agent, editable: false }, openChat: () => { opened++; } });
    assert.match(document.body.textContent, /Only Nova’s owner or an admin can add triggers here\..*ask Nova in Chat, in your own words\. It becomes a task that runs as you/);
    await click(button('Ask Nova in Chat'));
    assert.equal(opened, 1);
    assert.equal(document.querySelector('a.ui-button').getAttribute('href'), `/w/${w}/?view=activity&tab=planned`);
  }, { url, fetch: async path => path.endsWith('/session') ? response(session) : response({}) });
});

test('Recent work lists the agent’s requests as links and leads to all of Activity; a link can open its automations', async () => {
  const request = { id: 'reply', sequence: 1, chat: 'channel', thread: null, source_message: 'm', conversation_name: 'Orders', requester: 'me', requester_name: 'Ravi', summary: 'Check late orders', state: 'complete', latest_update: '', created_ms: 1000, can_cancel: false, cancellation: null, approvals: [] };
  await reactFixture(async ({ render, update }) => {
    const context = await import('../src/workspace-context.ts');
    await context.enterWorkspace();
    const { AgentWorkspace } = await import('../src/react/agents/workspace.tsx');
    const props = { agent, directory: { agents: [agent], role: 'Admin', default_model: 'fixture', system_prompt: '', providers: [] }, back() {}, openChat() {}, busy: false, saved: async () => {} };
    await render(AgentWorkspace, props);
    const work = document.querySelector('.profile-work');
    assert.equal(work.querySelector('h3').textContent, 'Recent work');
    assert.equal(work.querySelector('header a').getAttribute('href'), `/w/${w}/?view=activity&tab=history`);
    assert.equal(work.querySelector('li a').getAttribute('href'), `/w/${w}/?view=chat&conversation=channel&message=reply`);
    assert.equal(work.querySelector('button'), null, 'the agent page decides nothing');
    // ?section=automations (Activity › Upcoming's "Edit in Nova") opens the panel in the right-hand column.
    const { panelFor, sectionNamed } = await import('../src/react/agents/workspace-tabs.tsx');
    await render(AgentWorkspace, { ...props, agent: { ...agent, id: 'nova-2' }, start: panelFor(sectionNamed('automations')) });
    await update(async () => { await new Promise(resolve => setTimeout(resolve, 20)); });
    assert.equal(document.querySelector('.profile-panel').getAttribute('aria-label'), 'On its own');
    assert.equal(document.querySelector('.profile-work'), null, 'the panel takes Recent work’s place');
  }, { url, fetch: async path => {
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/requests')) return response({ requests: [request], next_before: null });
    if (path.includes('/event-triggers')) return response({ triggers: [], choices: [], apps: [], execution_available: true });
    return response({});
  } });
});
