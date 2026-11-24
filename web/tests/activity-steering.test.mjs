import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';

// Steering running work and routines proposed in Chat (Phase 4 AC6, T3).
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=activity&tab=history`;
const session = { user: { id: 'kavya', email: 'kavya@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Member' }] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const now = Date.now();
const details = (extra = {}) => ({ id: 'r1', chat: 'sales', thread: null, conversation_name: 'sales', conversation_kind: 'channel', in_conversation: true,
  agent: 'nova', agent_name: 'Nova', agent_avatar: null, kind: 'chat', requester: 'kavya', requester_name: 'Kavya Nair', summary: 'Send quotes to Acme',
  state: 'running', latest_update: '', created_ms: now, finished_ms: null, can_cancel: true, approvals: [], message: 'Send quotes to Acme', reply: '',
  steps: [], fix: null, disagreement: null, added: [{ text: 'Use the March price list', from: 'Kavya Nair', at: now, seen: true, carried: null }], ...extra });

test('the person who asked adds to a running request once; others can\'t', async () => {
  const writes = [];
  await reactFixture(async ({ render, change, update }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { RequestDetails } = await import('../src/react/activity/request-details.tsx');
    await render(() => createElement(RequestDetails, { reply: 'r1', close() {} }));
    const dialog = document.querySelector('dialog');
    assert.match(dialog.textContent, /Added while it ran.*Use the March price list.*the agent read it/);
    await change(dialog.querySelector('.activity-add-note textarea'), 'Also copy Ravi');
    await update(() => { const form = dialog.querySelector('.activity-add-note'); form.dispatchEvent(new window.Event('submit', { bubbles: true, cancelable: true }));
      form.dispatchEvent(new window.Event('submit', { bubbles: true, cancelable: true })); });
    assert.equal(writes.length, 1, 'once for a double submit');
    assert.ok(writes[0].path.endsWith('/activity/requests/r1/steer'));
    assert.deepEqual(writes[0].body, { text: 'Also copy Ravi' });
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (options.method === 'POST') { writes.push({ path, body: JSON.parse(options.body) }); return response({ added: true }); }
    return response(details());
  } });
  await reactFixture(async ({ render }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { RequestDetails } = await import('../src/react/activity/request-details.tsx');
    await render(() => createElement(RequestDetails, { reply: 'r1', close() {} }));
    assert.equal(document.querySelector('.activity-add-note'), null, 'not for someone else\'s request');
  }, { url, fetch: async path => path.endsWith('/session') ? response(session) : response(details({ requester: 'ravi', added: [] })) });
});
