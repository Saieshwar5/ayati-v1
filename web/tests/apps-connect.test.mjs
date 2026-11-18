import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Connect (designs/connector-screens 3): rules first, one request, sign-in on the app's page.
const w = '11111111-1111-4111-8111-111111111111';
const base = `http://localhost/w/${w}/?view=apps`;
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const session = response({ user: { id: 'me', email: 'me@test' }, workspaces: [{ id: w, name: 'Mehta', role: 'Admin' }] });
const op = (id, title, effect) => ({ id, title, effect });
const drive = { id: 'google_drive', name: 'Google Drive', account: null, provider: 'composio',
  operations: [op('google_drive.find_files', 'Find files', 'read'), op('google_drive.upload_file', 'Upload a file', 'write'), op('google_drive.delete_file', 'Delete a file', 'write')] };
const control = { servers: [drive], systems: [], connections: [], safety: [], people: [], viewer: 'me', execution_available: true };
const now = Math.floor(Date.now() / 1000);
const service = (server, name, extra = {}) => ({ id: `c-${server}`, server, name, kind: 'service', provider: 'composio', account: `${server}@mehta.test`, status: 'connected', rules: 0, agents: [],
  actions: [{ id: 'a', title: 'Find files', meaning: 'read', mode: 'allow' }, { id: 'b', title: 'Upload a file', meaning: 'change', mode: 'ask' }, { id: 'c', title: 'Delete a file', meaning: 'delete', mode: 'block' }],
  state: 'ok', last_used: null, connected: { at: now - 3600, by_name: 'Riya' }, ...extra });
const catalog = { categories: [], next_cursor: null, can_manage: true,
  apps: [{ id: 'outlook', name: 'Outlook', description: 'Email', category: 'email_calendar', kinds: ['read', 'sends_outside'], actions: 2, state: 'available', connection: null }] };

test('Connect shows the starting rules for the agent, sends once per click burst, retries with the same request and opens sign-in', async () => {
  const writes = []; let release;
  await reactFixture(async ({ render, update, click, button, navigation }) => {
    const { ConnectDialog } = await import('../src/react/apps/connect-dialog.tsx');
    await render(ConnectDialog, { app: { id: 'google_drive', name: 'Google Drive', kinds: ['read', 'change', 'delete'] }, agent: { id: 'nova', name: 'Nova' }, close() {}, connected() {} });
    const rules = [...document.querySelectorAll('.connect-rules li')].map(row => `${row.querySelector('span').firstChild.textContent} ${row.querySelector('[role=img]').getAttribute('aria-label')}`);
    assert.deepEqual(rules, ['Reads Starts as Allow', 'Changes Starts as Ask first', 'Deletes Starts as Block']);
    assert.match(document.querySelector('dialog[open]').textContent, /What Nova could do/);
    assert.match(document.querySelector('dialog[open]').textContent, /Only Nova can use this Google Drive account/);
    const go = () => button('Continue to Google Drive →');
    await update(() => { go().click(); go().click(); });
    assert.equal(writes.length, 1, 'one request per click burst');
    await update(() => release(response({ error: 'Response unavailable' }, 503)));
    assert.match(document.querySelector('dialog[open] [role=status]').textContent, /Response unavailable/);
    await click(go());
    await update(() => release(response({ redirect_url: 'https://connect.example.test/drive' })));
    assert.deepEqual(writes[0], writes[1], 'a retry reuses the same request');
    assert.equal(writes[0].app, 'google_drive'); assert.equal(writes[0].agent, 'nova'); assert.match(writes[0].request, /^[0-9a-f-]{36}$/);
    assert.deepEqual(navigation, ['https://connect.example.test/drive']);
  }, { url: base, fetch: async (path, options = {}) => {
    if (options.body) { writes.push(JSON.parse(options.body)); return new Promise(resolve => { release = resolve; }); }
    return response({});
  } });
});
