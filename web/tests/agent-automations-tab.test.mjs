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
const agent = { id: 'agent', name: 'Nova', role: 'Help customers', model: 'fixture', audience: 'everyone', revision: 3, editable: true, settings: {} };
const response = data => ({ ok: true, status: 200, json: async () => data });
const appTrigger = (extra = {}) => ({ app: 'leave', app_name: 'Leave', collection: 'requests', kind: 'added', instructions: 'Tell me who asked.', by: 'Ravi Kumar', manages: true, ...extra });

async function openPane(render) {
  const context = await import('../src/workspace-context.ts');
  await context.enterWorkspace();
  const { AutomationPane } = await import('../src/react/agents/automation/pane.tsx');
  await render(AutomationPane, { agent });
}

test('schedules and triggers sit in two groups, and app triggers are listed with who changes them', async () => {
  await reactFixture(async ({ render, update }) => {
    await openPane(render);
    await update(async () => { await new Promise(resolve => setTimeout(resolve, 20)); });
    const groups = [...document.querySelectorAll('.automation-group h4')].map(node => node.textContent);
    assert.deepEqual(groups.slice(0, 1), ['On a schedule']);
    assert.ok(groups.includes('When something happens'));
    assert.match(document.body.textContent, /No schedules yet\./);
    const rows = [...document.querySelectorAll('[aria-label^="Trigger:"]')];
    assert.deepEqual(rows.map(row => row.getAttribute('aria-label')), ['Trigger: New requests in Leave', 'Trigger: Changed stock in Stock']);
    assert.match(rows[0].textContent, /Added from Leave.*Tell me who asked\..*Set up by Ravi Kumar.*Change or stop it on Apps › Leave/);
    assert.match(rows[1].textContent, /Its builder or an admin changes it on Apps › Stock/);
    assert.equal(rows[0].querySelector('a').getAttribute('href'), `/w/${w}/?view=apps`);
    assert.doesNotMatch(document.body.textContent, /No triggers yet/);
  }, { url, fetch: async path => {
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/event-triggers')) return response({ triggers: [], choices: [], apps: [], execution_available: true,
      app_triggers: [appTrigger(), appTrigger({ app: 'stock', app_name: 'Stock', collection: 'stock', kind: 'changed', manages: false, by: null })] });
    if (path.includes('/schedules')) return response({ schedules: [], conversations: [], more_conversations: false, execution_available: true });
    return response({});
  } });
});

test('old and linked tab names open Automations', async () => {
  const { sectionNamed, agentSectionUrl } = await import('../src/react/agents/workspace-tabs.tsx');
  assert.equal(sectionNamed('Schedules'), 'Automations', 'a saved "Schedules" tab still opens');
  assert.equal(sectionNamed('automations'), 'Automations');
  assert.equal(sectionNamed('tools'), 'Tools');
  assert.equal(sectionNamed('nonsense'), undefined);
  assert.match(agentSectionUrl('a b', 'Automations'), /\?view=agents&selected=a%20b&section=automations$/);
});
