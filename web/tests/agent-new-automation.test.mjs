// "New automation" (designs/automations-simplified 2): one button, then At a time
// (asked for in Chat, where it becomes a task) or When something happens (a form).
import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const session = { user: { id: 'owner', email: 'owner@example.test', display_name: 'Ravi Kumar' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };
const agent = { id: 'agent', name: 'Nova', role: 'Help customers', model: 'fixture', audience: 'everyone', revision: 3, editable: true, settings: {} };
const response = data => ({ ok: true, status: 200, json: async () => data });
const events = extra => ({ triggers: [], choices: [], apps: [], execution_available: true, ...extra });

async function openPane(render, update) {
  const context = await import('../src/workspace-context.ts');
  await context.enterWorkspace();
  const { AutomationPane } = await import('../src/react/agents/automation/pane.tsx');
  await render(AutomationPane, { agent });
  await update(async () => { await new Promise(resolve => setTimeout(resolve, 20)); });
}
const kind = label => [...document.querySelectorAll('.automation-kind')].find(node => node.querySelector('strong').textContent === label);

test('one button: At a time asks the agent in Chat, where it becomes a task', async () => {
  let opened = 0;
  await reactFixture(async ({ render, update, click, button }) => {
    const context = await import('../src/workspace-context.ts');
    await context.enterWorkspace();
    const { AutomationPane } = await import('../src/react/agents/automation/pane.tsx');
    await render(AutomationPane, { agent, openChat: () => { opened++; } });
    await update(async () => { await new Promise(resolve => setTimeout(resolve, 20)); });
    assert.deepEqual([...document.querySelectorAll('button')].filter(b => /Create event trigger|happens in an app/.test(b.textContent)), [], 'one way in');
    await click(button('New automation'));
    assert.match(document.querySelector('dialog').textContent, /New automation for Nova.*When should Nova start this work\?.*At a time.*ask Nova in Chat and it becomes a task.*When something happens/);
    await click(kind('At a time'));
    assert.equal(opened, 1, 'Chat opens');
    assert.equal(document.querySelector('dialog'), null);
  }, { url, fetch: async path => {
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/event-triggers')) return response(events());
    return response({});
  } });
});

test('When something happens opens the app form, or asks which kind when reviewed sources exist too, or says why it can’t', async () => {
  const run = async (extra, check) => reactFixture(async helpers => {
    const { render, update, click, button } = helpers;
    await openPane(render, update);
    await click(button('New automation'));
    await click(kind('When something happens'));
    await check(helpers);
  }, { url, fetch: async path => {
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/event-triggers')) return response(events(extra));
    return response({ events: [] });
  } });
  await run({}, async () => {
    assert.match(document.querySelector('dialog').textContent, /New trigger: when something happens in an app/);
  });
  await run({ choices: [{ source: { id: 's', name: 'Orders', event_type: 'order', fields: [], binding: 'b' }, chat: 'c', chat_name: 'Orders', visibility: 'public', authority: { connection_revision: 1, read_revision: 1, delegation_revision: null } }] }, async ({ click }) => {
    assert.match(document.querySelector('dialog').textContent, /In a connected app.*From a reviewed event source/);
    await click(kind('From a reviewed event source'));
    assert.match(document.querySelector('dialog').textContent, /New trigger from a reviewed source/);
  });
  await run({ execution_available: false }, async ({ click, button }) => {
    assert.match(document.querySelector('dialog').textContent, /Triggers need background work that this server doesn’t run/);
    await click(button('Done'));
    assert.equal(document.querySelector('dialog'), null);
  });
});
