// The agent's Automations tab (formerly Schedules): schedules and triggers in two
// groups, with triggers set up from an app listed beside the agent's own.
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

test('schedules and triggers sit in two groups', async () => {
  await reactFixture(async ({ render, update }) => {
    await openPane(render);
    await update(async () => { await new Promise(resolve => setTimeout(resolve, 20)); });
    const groups = [...document.querySelectorAll('.automation-group h4')].map(node => node.textContent);
    assert.deepEqual(groups.slice(0, 1), ['On a schedule']);
    assert.ok(groups.includes('When something happens'));
    assert.match(document.body.textContent, /No schedules yet\./);
    assert.match(document.body.textContent, /No triggers yet\./);
  }, { url, fetch: async path => {
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/event-triggers')) return response({ triggers: [], choices: [], apps: [], execution_available: true });
    if (path.includes('/schedules')) return response({ schedules: [], conversations: [], more_conversations: false, execution_available: true });
    return response({});
  } });
});

test('old and linked tab names open Automations', async () => {
  const { sectionNamed, agentSectionUrl } = await import('../src/react/agents/workspace-tabs.tsx');
  assert.equal(sectionNamed('Schedules'), 'Automations', 'a saved "Schedules" tab still opens');
  assert.equal(sectionNamed('automations'), 'Automations');
  assert.equal(sectionNamed('tools'), 'Connections', 'a link to the old Tools tab opens Connections');
  assert.equal(sectionNamed('environment'), 'Settings');
  assert.equal(sectionNamed('activity'), 'Overview');
  assert.equal(sectionNamed('nonsense'), undefined);
  assert.match(agentSectionUrl('a b', 'Automations'), /\?view=agents&selected=a%20b&section=automations$/);
});

test('people who only talk to the agent are pointed to Chat, and a manager sees how many schedules stay private', async () => {
  let opened = 0;
  await reactFixture(async ({ render, button, click }) => {
    const context = await import('../src/workspace-context.ts');
    await context.enterWorkspace();
    const { AutomationPane } = await import('../src/react/agents/automation/pane.tsx');
    await render(AutomationPane, { agent: { ...agent, editable: false }, openChat: () => { opened++; } });
    assert.match(document.body.textContent, /Only Nova’s owner or an admin can add schedules and triggers here\..*ask Nova in Chat, in your own words/);
    await click(button('Ask Nova in Chat'));
    assert.equal(opened, 1);
    assert.equal(document.querySelector('a.ui-button').getAttribute('href'), `/w/${w}/?view=activity&tab=upcoming`);
  }, { url, fetch: async path => path.endsWith('/session') ? response(session) : response({}) });
  await reactFixture(async ({ render, update }) => {
    await openPane(render);
    await update(async () => { await new Promise(resolve => setTimeout(resolve, 20)); });
    assert.match(document.body.textContent, /2 more schedules post in conversations you’re not in, so they stay private\. Pausing Nova pauses every schedule\./);
    assert.doesNotMatch(document.body.textContent, /No schedules yet/);
  }, { url, fetch: async path => {
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/event-triggers')) return response({ triggers: [], choices: [], apps: [], execution_available: true });
    if (path.includes('/schedules')) return response({ schedules: [], elsewhere: 2, conversations: [], more_conversations: false, execution_available: true });
    return response({});
  } });
});

test('an agent’s activity opens on Overview when asked, as a filtered view that links to all of Activity', async () => {
  await reactFixture(async ({ render, click, button }) => {
    const context = await import('../src/workspace-context.ts');
    await context.enterWorkspace();
    const { AgentWorkspace } = await import('../src/react/agents/workspace.tsx');
    await render(AgentWorkspace, { agent, directory: { agents: [agent], role: 'Admin', default_model: 'fixture', system_prompt: '', providers: [] },
      back() {}, openChat() {}, busy: false, section: 'Overview', selectSection() {}, saved: async () => {} });
    assert.equal(document.querySelector('.agent-filtered-note'), null, 'the list is read only when asked for');
    await click(button('Show activity'));
    const note = document.querySelector('.agent-filtered-note');
    assert.match(note.textContent, /Showing Nova only: the same list as Activity, filtered to one agent\. Open all activity/);
    assert.equal(note.querySelector('a').getAttribute('href'), `/w/${w}/?view=activity&tab=history`);
  }, { url, fetch: async path => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chat-directory')) return response({ users: [], agents: [] });
    if (path.endsWith('/chats')) return response({ chats: [] });
    if (path.includes('/activity/totals')) return response({ requests: 0, done: 0, failed: 0, review: 0, waiting: 0, automatic: 0, approved: 0, people: 0, channels: 0, schedule_runs: 0, per_day: [], by_agent: [] });
    return response({ items: [], next_before: null });
  } });
});
