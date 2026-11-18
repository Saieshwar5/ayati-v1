import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Allowed apps (schema 134): members add the apps an admin allowed and ask for
// the others; admins answer requests and allow or switch off apps in Settings.
const w = '11111111-1111-4111-8111-111111111111';
const base = `/api/identity/workspaces/${w}`;
const url = `http://localhost/w/${w}/?view=settings&section=connections`;
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const tile = (id, name, state, extra = {}) => ({ id, name, description: `Tools for ${name}`, category: 'files', kinds: ['read'], actions: 2, state, connection: null, ...extra });
const page = apps => ({ categories: [{ id: 'files', name: 'Files', count: apps.length }], next_cursor: null, can_manage: true, apps });
const tileOf = name => [...document.querySelectorAll('.catalog-tile')].find(node => node.querySelector('strong').textContent === name);

test('a member adds allowed apps, asks once for an undecided one and sees why a blocked one is off', async () => {
  const writes = [];
  let apps = [tile('gmail', 'Gmail', 'available'), tile('sheets', 'Google Sheets', 'ask'), tile('drive', 'Google Drive', 'blocked', { note: 'Files stay on the shared drive' })];
  await reactFixture(async ({ render, click, change, update }) => {
    const { CatalogDialog } = await import('../src/react/apps/catalog-dialog.tsx');
    const picked = [];
    await render(CatalogDialog, { agent: 'nova', close() {}, connect: app => picked.push(app.id) });
    assert.ok(tileOf('Gmail').querySelector('button[aria-label="Connect Gmail"]'));
    assert.match(tileOf('Google Drive').textContent, /Not available here.*An admin switched it off: Files stay on the shared drive/);
    assert.equal(tileOf('Google Drive').querySelector('button'), null, 'a blocked app can’t be added or asked for');
    const sheets = tileOf('Google Sheets');
    assert.equal(sheets.querySelector('button[aria-label="Connect Google Sheets"]'), null);
    assert.match(sheets.textContent, /Needs an admin’s yes first/);
    await click(sheets.querySelector('button[aria-label="Ask an admin for Google Sheets"]'));
    const dialog = [...document.querySelectorAll('dialog')].find(node => /Ask for Google Sheets/.test(node.textContent));
    await change(dialog.querySelector('textarea'), '  Monthly report  ');
    apps = [apps[0], tile('sheets', 'Google Sheets', 'requested'), apps[2]];
    const send = [...dialog.querySelectorAll('button')].find(node => node.textContent === 'Send request');
    await update(() => { send.click(); send.click(); });
    assert.equal(writes.length, 1, 'one request despite a double click');
    assert.equal(writes[0].path, `${base}/apps/requests`);
    assert.deepEqual({ ...writes[0].body, id: undefined }, { id: undefined, connector: 'sheets', agent: 'nova', reason: 'Monthly report' });
    assert.match(writes[0].body.id, /^[0-9a-f-]{36}$/);
    assert.match(tileOf('Google Sheets').textContent, /Asked · waiting for an admin/);
    assert.equal(tileOf('Google Sheets').querySelector('button'), null);
    assert.deepEqual(picked, []);
  }, { url, fetch: async (path, options = {}) => {
    if (options.body) { writes.push({ path, body: JSON.parse(options.body) }); return response({ id: 'r', state: 'pending' }); }
    return response(page(apps));
  } });
});

test('a failed request keeps the reason so it can be sent again', async () => {
  let fail = true; const writes = [];
  await reactFixture(async ({ render, click, change }) => {
    const { RequestAppDialog } = await import('../src/react/apps/request-app-dialog.tsx');
    let sent = 0;
    await render(RequestAppDialog, { app: { id: 'sheets', name: 'Google Sheets' }, agent: 'nova', close() {}, sent: () => sent++ });
    await change(document.querySelector('textarea'), 'Monthly report');
    const send = () => [...document.querySelectorAll('button')].find(node => node.textContent === 'Send request');
    await click(send());
    assert.match(document.querySelector('dialog [role="status"]').textContent, /You already asked/);
    assert.equal(document.querySelector('textarea').value, 'Monthly report');
    fail = false;
    await click(send());
    assert.equal(sent, 1);
    assert.equal(writes[0].id, writes[1].id, 'the retry is the same request');
  }, { url, fetch: async (path, options) => { writes.push(JSON.parse(options.body)); return fail ? response({ error: 'You already asked; an admin decides' }, 409) : response({ id: 'r', state: 'pending' }); } });
});

const allowed = (id, name, allowance, extra = {}) => ({ id, name, description: '', category: 'files', allowance, note: '', revision: allowance === 'undecided' ? 0 : 3, decided: null, connections: 0, waiting: 0, ...extra });
const asked = { id: 'req-1', kind: 'suggestion', connector: 'sheets', name: 'Google Sheets', reason: 'Monthly report', created: 1763283720, requester: 'Priya Shah', agent: 'fin', agent_name: 'Finance Desk' };
function settings(state, writes) {
  return async (path, options = {}) => {
    if (options.body) { writes.push({ path: path.replace(base, ''), body: JSON.parse(options.body) }); return state.fail ? response({ error: 'Settings changed; reload before saving' }, 409) : response({ saved: true, state: 'approved' }); }
    if (path.endsWith('/apps/allowed')) return response({ apps: state.apps });
    return response({ requests: state.requests, mine: [], can_decide: true });
  };
}
const row = text => [...document.querySelectorAll('.settings-row')].find(node => node.textContent.includes(text));

test('an admin answers a request: Allow sends one approval, Decline carries a note', async () => {
  const writes = [], state = { apps: [allowed('sheets', 'Google Sheets', 'undecided', { waiting: 1 })], requests: [asked] };
  await reactFixture(async ({ render, click, change, update }) => {
    const { AllowedApps } = await import('../src/react/settings/allowed-apps.tsx');
    let changed = 0;
    await render(AllowedApps, { changed: () => changed++ });
    const request = row('Priya Shah asks for Google Sheets');
    assert.match(request.textContent, /for Finance Desk · “Monthly report”/);
    await click(request.querySelector('button[aria-label="Decline Google Sheets for Priya Shah"]'));
    const dialog = [...document.querySelectorAll('dialog')].find(node => /Decline Google Sheets/.test(node.textContent));
    assert.match(dialog.textContent, /Priya Shah sees your answer on Home/);
    await change(dialog.querySelector('textarea'), 'Use the finance export');
    await click([...dialog.querySelectorAll('button')].find(node => node.textContent === 'Decline'));
    assert.deepEqual(writes, [{ path: '/apps/requests/req-1', body: { action: 'decline', note: 'Use the finance export' } }]);
    assert.match(document.body.textContent, /Declined Priya Shah’s request\./);
    const allow = row('Priya Shah asks for Google Sheets').querySelector('button[aria-label="Allow Google Sheets for Priya Shah"]');
    state.requests = []; state.apps = [allowed('sheets', 'Google Sheets', 'allowed')];
    await update(() => { allow.click(); allow.click(); });
    assert.equal(writes.length, 2, 'one approval despite a double click');
    assert.deepEqual(writes[1], { path: '/apps/requests/req-1', body: { action: 'approve', note: '' } });
    assert.match(document.body.textContent, /Google Sheets is allowed\. Priya Shah can add it now\./);
    assert.equal(document.querySelector('[aria-label="App requests"]'), null, 'nothing waits any more');
    assert.ok(changed >= 2, 'the connections list below is told');
  }, { url, fetch: settings(state, writes) });
});

test('an admin allows an undecided app and switches one off with a warning and a note', async () => {
  const writes = [], state = { requests: [], apps: [
    allowed('drive', 'Google Drive', 'undecided'),
    allowed('gmail', 'Gmail', 'allowed', { connections: 2, decided: { at: 1763283720, by_name: 'Asha Rao' } }),
    allowed('sheets', 'Google Sheets', 'blocked', { note: 'Not for now' })] };
  await reactFixture(async ({ render, click, change }) => {
    const { AllowedApps } = await import('../src/react/settings/allowed-apps.tsx');
    await render(AllowedApps);
    assert.match(row('Google Drive').textContent, /Not decided: members ask first/);
    assert.match(row('Gmail').textContent, /Allowed · .* by Asha Rao · 2 connections/);
    assert.match(row('Google Sheets').textContent, /Switched off · Not for now/);
    assert.equal(row('Gmail').querySelector('button[aria-label="Allow Gmail"]'), null);
    assert.equal(row('Google Sheets').querySelector('button[aria-label="Switch off Google Sheets"]'), null);
    await click(row('Google Drive').querySelector('button[aria-label="Allow Google Drive"]'));
    assert.deepEqual(writes[0], { path: '/apps/allowed', body: { app: 'drive', allowed: true, note: '', revision: 0 } });
    await click(row('Gmail').querySelector('button[aria-label="Switch off Gmail"]'));
    const dialog = () => [...document.querySelectorAll('dialog')].find(node => /Switch off Gmail/.test(node.textContent));
    assert.match(dialog().textContent, /2 connections of Gmail end at once/);
    await change(dialog().querySelector('textarea'), 'Mail stays in the shared inbox');
    state.fail = true;
    const go = () => [...dialog().querySelectorAll('button')].find(node => node.textContent === 'Switch off');
    await click(go());
    assert.match(dialog().querySelector('[role="status"]').textContent, /Settings changed; reload before saving/);
    assert.equal(dialog().querySelector('textarea').value, 'Mail stays in the shared inbox', 'a failed save keeps the note');
    state.fail = false;
    await click(go());
    assert.deepEqual(writes.at(-1), { path: '/apps/allowed', body: { app: 'gmail', allowed: false, note: 'Mail stays in the shared inbox', revision: 3 } });
    assert.equal(dialog(), undefined);
    assert.match(document.body.textContent, /Gmail is switched off\./);
  }, { url, fetch: settings(state, writes) });
});

test('Home tells admins what waits and tells a person the answer to what they asked', async () => {
  const mine = (id, name, state, extra = {}) => ({ id, connector: id, name, state, note: '', created: 1, decided: 2, agent: 'nova', agent_name: 'Nova', ...extra });
  const show = async (data, check) => reactFixture(async ({ render }) => {
    const { AppRequestNotices } = await import('../src/react/apps/app-request-notices.tsx');
    await render(AppRequestNotices);
    check(document.querySelector('[aria-label="App requests"]'));
  }, { url, fetch: async () => data ? response(data) : response({ error: 'Unavailable' }, 500) });
  await show({ requests: [asked, { ...asked, id: 'req-2' }], mine: [], can_decide: true }, section => {
    assert.match(section.textContent, /2 requests for an app or MCP server are waiting for your answer\./);
    assert.match(section.querySelector('a').getAttribute('href'), /view=settings&section=connections$/);
  });
  await show({ requests: [], can_decide: false, mine: [mine('gmail', 'Gmail', 'approved'), mine('sheets', 'Google Sheets', 'declined', { note: 'Use the finance export' }), mine('drive', 'Google Drive', 'pending')] }, section => {
    assert.match(section.textContent, /Gmail is now allowed in this workspace\./);
    const link = section.querySelector('a');
    assert.equal(link.textContent, 'Add it to Nova');
    assert.match(link.getAttribute('href'), /view=agents&selected=nova&section=connections$/);
    assert.match(section.textContent, /Your request for Google Sheets was declined\. “Use the finance export”/);
    assert.doesNotMatch(section.textContent, /Google Drive/, 'a waiting request says nothing yet');
  });
  await show({ requests: [], mine: [], can_decide: false }, section => assert.equal(section, null));
  await show(null, section => assert.equal(section, null, 'a failed read stays quiet'));
});

test('an admin allows or declines an MCP server a member connected', async () => {
  const writes = [];
  const system = { id: 'sys-1', name: 'Billing', url: 'https://billing.example.test/mcp', tools: 3, revision: 1, created_ms: 1, requester: 'Priya Shah', agent: 'fin', agent_name: 'Finance Desk' };
  const state = { systems: [system, { ...system, id: 'sys-2', name: 'Stock' }] };
  await reactFixture(async ({ render, update }) => {
    const { AllowedApps } = await import('../src/react/settings/allowed-apps.tsx');
    await render(AllowedApps);
    const billing = row('Priya Shah connected the MCP server Billing');
    assert.match(billing.textContent, /for Finance Desk · https:\/\/billing\.example\.test\/mcp · 3 tools/);
    const allow = billing.querySelector('button[aria-label="Allow the MCP server Billing"]');
    state.systems = [state.systems[1]];
    await update(() => { allow.click(); allow.click(); });
    assert.deepEqual(writes, [{ path: '/agent-profiles/fin/systems/sys-1/state', body: { revision: 1, change: 'allow' } }]);
    assert.match(document.body.textContent, /Billing is allowed\. Finance Desk can use it now\./);
    state.systems = [];
    await update(() => row('MCP server Stock').querySelector('button[aria-label="Decline the MCP server Stock"]').click());
    assert.deepEqual(writes[1], { path: '/agent-profiles/fin/systems/sys-2/state', body: { revision: 1, change: 'decline' } });
    assert.equal(document.querySelector('[aria-label="App requests"]'), null);
  }, { url, fetch: async (path, options = {}) => {
    if (options.body) { writes.push({ path: path.replace(base, ''), body: JSON.parse(options.body) }); return response({ id: 's', state: 'allow' }); }
    if (path.endsWith('/apps/allowed')) return response({ apps: [], systems: state.systems });
    return response({ requests: [], mine: [], can_decide: true });
  } });
});
