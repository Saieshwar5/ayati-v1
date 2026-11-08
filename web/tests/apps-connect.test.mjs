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

test('Connect shows the starting rules, sends once per click burst, retries with the same request and opens sign-in', async () => {
  const writes = []; let release;
  await reactFixture(async ({ dom, render, update, click, button, navigation }) => {
    globalThis.sessionStorage = dom.window.sessionStorage;
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { ConnectDialog } = await import('../src/react/apps/connect-dialog.tsx');
    await render(ConnectDialog, { app: { id: 'google_drive', name: 'Google Drive', kinds: ['read', 'change', 'delete'] }, close() {}, connected() {} });
    const rules = [...document.querySelectorAll('.connect-rules li')].map(row => `${row.querySelector('span').textContent} ${row.querySelector('[role=img]').getAttribute('aria-label')}`);
    assert.deepEqual(rules, ['Find filesReads Starts as Allow', 'Upload a fileChanges Starts as Ask first', 'Delete a fileDeletes Starts as Block']);
    const go = () => button('Continue to Google Drive →');
    await update(() => { go().click(); go().click(); });
    assert.equal(writes.length, 1, 'one request per click burst');
    await update(() => release(response({ error: 'Response unavailable' }, 503)));
    assert.match(document.querySelector('dialog[open] [role=status]').textContent, /Response unavailable/);
    await click(go());
    await update(() => release(response({ redirect_url: 'https://connect.example.test/drive' })));
    assert.deepEqual(writes[0], writes[1], 'a retry reuses the same request');
    assert.equal(writes[0].app, 'google_drive'); assert.match(writes[0].request, /^[0-9a-f-]{36}$/);
    assert.deepEqual(navigation, ['https://connect.example.test/drive']);
    assert.equal(sessionStorage.getItem(`apps.connecting.me.${w}`), 'google_drive', 'the app is remembered for the return');
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return session;
    if (options.body) { writes.push(JSON.parse(options.body)); return new Promise(resolve => { release = resolve; }); }
    return response(control);
  } });
  delete globalThis.sessionStorage;
});

test('Back from sign-in the dialog names the account and offers next steps, once', async () => {
  await reactFixture(async ({ dom, render, click, button }) => {
    globalThis.sessionStorage = dom.window.sessionStorage;
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    sessionStorage.setItem(`apps.connecting.me.${w}`, 'google_drive');
    const { AppsPage } = await import('../src/react/apps/apps-page.tsx');
    await render(AppsPage, { href: dom.window.location.href });
    const dialog = document.querySelector('dialog[open]');
    assert.equal(dialog.querySelector('h2').textContent, 'Google Drive is connected');
    assert.match(dialog.textContent, /Connected as google_drive@mehta\.test\. Reading is allowed; changing asks first; deleting is blocked\./);
    assert.ok(button('Choose agents'));
    assert.doesNotMatch(dom.window.location.href, /connected=1/, 'a reload does not announce it again');
    assert.equal(sessionStorage.getItem(`apps.connecting.me.${w}`), null);
    await click(button('Change rules'));
    assert.equal(document.querySelector('dialog[open]'), null);
    assert.equal(document.querySelector('.app-panel h2').textContent, 'Google Drive');
  }, { url: `${base}&manage=1&connected=1`, fetch: async path => path.endsWith('/session') ? session
    : path.endsWith('/control/connections') ? response(control)
    : response({ apps: [service('gmail', 'Gmail', { connected: { at: now - 60, by_name: 'Riya' } }), service('google_drive', 'Google Drive')], systems: [], can_manage: true, requests_waiting: 0 }) });
  delete globalThis.sessionStorage;
});

test('A failed sign-in says nothing was connected and offers to try again', async () => {
  await reactFixture(async ({ dom, render, click, button }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { AppsPage } = await import('../src/react/apps/apps-page.tsx');
    await render(AppsPage, { href: dom.window.location.href });
    assert.equal(document.querySelector('dialog[open]'), null);
    assert.match(document.querySelector('.apps-notice[role=alert]').textContent, /The sign-in didn’t finish, so nothing was connected\./);
    await click(button('Try again'));
    assert.equal(document.querySelector('dialog[open] h2').textContent, 'Add app');
    assert.equal(document.querySelector('.apps-notice'), null);
  }, { url: `${base}&manage=1&connect_error=1`, fetch: async path => path.endsWith('/session') ? session
    : path.endsWith('/control/connections') ? response(control) : path.includes('/apps/catalog') ? response(catalog)
    : response({ apps: [service('gmail', 'Gmail')], systems: [], can_manage: true, requests_waiting: 0 }) });
});

test('An app verified at once goes straight to the connected dialog from the catalog', async () => {
  let connected = false; const writes = [];
  await reactFixture(async ({ render, click, button }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { AppsPage } = await import('../src/react/apps/apps-page.tsx');
    await render(AppsPage, { href: base });
    await click(button('Add app'));
    await click(button('Connect Outlook'));
    assert.equal(document.querySelector('dialog[open] h2').textContent, 'Connect Outlook');
    assert.match(document.querySelector('.connect-rules').textContent, /ReadsEvery action of this kind.*Sends outsideEvery action of this kind/, 'kinds stand in when the app’s actions are not listed');
    await click(button('Continue to Outlook →'));
    assert.equal(writes.length, 1);
    assert.equal(document.querySelector('dialog[open] h2').textContent, 'Outlook is connected');
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return session;
    if (options.body) { writes.push(JSON.parse(options.body)); connected = true; return response({ connection: 'c-outlook' }); }
    if (path.endsWith('/control/connections')) return response(control);
    if (path.includes('/apps/catalog')) return response(catalog);
    return response({ apps: connected ? [service('gmail', 'Gmail'), service('outlook', 'Outlook')] : [service('gmail', 'Gmail')], systems: [], can_manage: true, requests_waiting: 0 });
  } });
});
