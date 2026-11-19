import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Add more apps (schema 135): an admin searches the library, chooses an app's
// actions and adds it; an added app can be changed or removed.
const w = '11111111-1111-4111-8111-111111111111';
const base = `/api/identity/workspaces/${w}`;
const url = `http://localhost/w/${w}/?view=settings&section=connections`;
const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });
const allowed = (id, name, extra = {}) => ({ id, name, description: '', category: 'sales', allowance: 'allowed', note: '', revision: 1, decided: null, connections: 0, waiting: 0, added: null, ...extra });
const action = (tool, title, extra = {}) => ({ tool, title, description: `Does ${title}`, effect: 'write', may_read: false, available: true, reason: null, ...extra });
const library = [
  { app: 'hubspot', id: 'hubspot', name: 'HubSpot', description: 'Customers and deals', actions: 354, state: 'addable', reason: null },
  { app: 'docker_hub', id: 'docker_hub', name: 'Docker Hub', description: 'Container images', actions: 12, state: 'unsupported', reason: 'Needs an API key or its own sign-in setup; not supported yet' },
  { app: 'gmail', id: 'gmail', name: 'Gmail', description: 'Email', actions: 61, state: 'builtin', reason: null }];
const first = [action('HUBSPOT_SEARCH_CONTACTS', 'Search contacts', { effect: 'read', may_read: true }), action('HUBSPOT_CREATE_NOTE', 'Create note'),
  action('HUBSPOT_CREATE_CONTACTS', 'Create contacts in bulk', { effect: null, available: false, reason: 'It needs an input Cuckoo does not offer yet, such as a list of records.' })];
const later = [action('HUBSPOT_PING', 'Check the account', { may_read: true })];

function server(state, reads, writes) {
  return async (path, options = {}) => {
    const short = path.replace(base, '');
    if (options.body) {
      writes.push({ path: short, body: JSON.parse(options.body) });
      if (state.fail) return response({ error: 'Settings changed; reload before saving' }, 409);
      return response(short.endsWith('/remove') ? { removed: true, ended: 2 } : { saved: true, app: 'hubspot', name: 'HubSpot', ended: state.ended ?? 0, provider_revoked: true });
    }
    reads.push(short);
    if (short === '/apps/allowed') return response({ apps: state.apps, library: state.library ?? true });
    if (short.startsWith('/apps/library?')) return response({ apps: library.filter(app => app.id.includes(new URL(path, 'http://x').searchParams.get('q'))) });
    if (short.includes('/actions?')) {
      const page = new URL(path, 'http://x').searchParams.get('page');
      return response({ app: 'hubspot', id: 'hubspot', name: 'HubSpot', description: 'Customers and deals', total: 354, actions: page ? later : first, next: page ? null : 'Mi0y', added: null });
    }
    return response({ requests: [], mine: [], can_decide: true });
  };
}
const row = (text, scope = document) => [...scope.querySelectorAll('li')].find(node => node.textContent.includes(text));
const dialog = title => [...document.querySelectorAll('dialog')].find(node => node.querySelector('h2')?.textContent === title);
const named = (scope, label) => [...scope.querySelectorAll('button')].find(node => node.textContent === label || node.getAttribute('aria-label') === label);

test('an admin searches the library, reviews an app’s actions and adds it once', async () => {
  const reads = [], writes = [], state = { apps: [allowed('gmail', 'Gmail')] };
  await reactFixture(async ({ render, click, change, submit, update }) => {
    const { AllowedApps } = await import('../src/react/settings/allowed-apps.tsx');
    let changed = 0;
    await render(AllowedApps, { changed: () => changed++ });
    await click(named(document, 'Add more apps'));
    const search = () => dialog('Add more apps');
    await change(search().querySelector('input[type="search"]'), 'hub');
    assert.ok(!reads.some(path => path.includes('q=hub')), 'typing alone asks the library nothing');
    await submit(search().querySelector('form'));
    assert.ok(reads.includes('/apps/library?q=hub'));
    // Only an app the library signs in itself can be chosen; the others say why not.
    assert.match(row('Docker Hub', search()).textContent, /Needs an API key.*Not supported yet/);
    assert.equal(!!named(row('Docker Hub', search()), 'Choose actions for Docker Hub'), false);
    await click(named(search(), 'Choose actions for HubSpot'));
    const review = () => dialog('Add HubSpot');
    const box = title => row(title, review()).querySelector('input[type="checkbox"]');
    assert.match(review().textContent, /0 of 20 chosen/);
    assert.equal(named(review(), 'Add HubSpot').disabled, true, 'nothing to add yet');
    assert.equal(box('Create contacts in bulk').disabled, true);
    assert.match(row('Create contacts in bulk', review()).textContent, /Not available\. It needs an input Cuckoo does not offer yet/);
    await click(box('Search contacts'));
    await click(box('Create note'));
    // What the library says changes something can't be called a read.
    const effect = title => row(title, review()).querySelector('select');
    assert.equal(effect('Search contacts').value, 'read');
    assert.equal(effect('Create note').value, 'write');
    assert.equal(effect('Create note').querySelector('option[value="read"]').disabled, true);
    // A later page adds to the list; the admin may call an unlabelled action a read.
    await click(named(review(), 'Show more actions'));
    assert.ok(reads.some(path => path.includes('page=Mi0y')));
    assert.equal(!!named(review(), 'Show more actions'), false);
    await click(box('Check the account'));
    await change(effect('Check the account'), 'read');
    assert.match(review().textContent, /3 of 20 chosen/);
    // A failed save keeps the choices; the retry sends them once despite a double click.
    state.fail = true;
    await click(named(review(), 'Add HubSpot'));
    assert.match(review().querySelector('.ui-dialog-footer [role="status"]').textContent, /Settings changed; reload before saving/);
    assert.equal(box('Create note').checked, true);
    state.fail = false; state.apps = [allowed('gmail', 'Gmail'), allowed('hubspot', 'HubSpot')];
    const go = named(review(), 'Add HubSpot');
    await update(() => { go.click(); go.click(); });
    assert.equal(writes.length, 2, 'one failed save and one saved');
    assert.deepEqual(writes[1], { path: '/apps/added', body: { app: 'hubspot', revision: 0, actions: [
      { tool: 'HUBSPOT_SEARCH_CONTACTS', effect: 'read' }, { tool: 'HUBSPOT_CREATE_NOTE', effect: 'write' }, { tool: 'HUBSPOT_PING', effect: 'read' }] } });
    assert.equal(!!review(), false);
    assert.match(document.body.textContent, /HubSpot was added and is allowed\. People can add it to their agents now\./);
    assert.ok(changed >= 1, 'the connections list below is told');
  }, { url, fetch: server(state, reads, writes) });
});

test('an admin changes an added app’s actions with a warning, and removes it after confirming', async () => {
  const added = { revision: 4, toolkit: 'hubspot', added: { at: 1763283720, by_name: 'Asha Rao' },
    actions: [{ tool: 'HUBSPOT_SEARCH_CONTACTS', title: 'Search contacts', effect: 'read' }, { tool: 'HUBSPOT_OLD', title: 'An older action', effect: 'write' }] };
  const reads = [], writes = [], state = { ended: 2, apps: [allowed('gmail', 'Gmail'), allowed('hubspot', 'HubSpot', { connections: 2, added })] };
  await reactFixture(async ({ render, click }) => {
    const { AllowedApps } = await import('../src/react/settings/allowed-apps.tsx');
    await render(AllowedApps);
    const hubspot = () => [...document.querySelectorAll('.settings-row')].find(node => node.textContent.includes('HubSpot'));
    assert.match(hubspot().textContent, /Added from the library · 2 actions · Allowed · 2 connections/);
    const gmail = [...document.querySelectorAll('.settings-row')].find(node => node.textContent.includes('Gmail'));
    assert.equal(!!named(gmail, 'Remove Gmail'), false, 'a built-in app is switched off, never removed');
    await click(named(hubspot(), 'Change the actions of HubSpot'));
    const review = () => dialog('Change HubSpot');
    assert.match(review().textContent, /Saving ends 2 connections of HubSpot\. Their owners connect it again/);
    assert.equal(row('Search contacts', review()).querySelector('input').checked, true, 'the saved choices are ticked');
    // A chosen action outside the list shown is still listed, and can be dropped.
    assert.match(review().querySelector('[aria-label="Also chosen"]').textContent, /An older action/);
    await click(named(review(), 'Remove An older action'));
    await click(row('Create note', review()).querySelector('input'));
    await click(named(review(), 'Save changes'));
    assert.deepEqual(writes[0], { path: '/apps/added', body: { app: 'hubspot', revision: 4, actions: [
      { tool: 'HUBSPOT_SEARCH_CONTACTS', effect: 'read' }, { tool: 'HUBSPOT_CREATE_NOTE', effect: 'write' }] } });
    assert.match(document.body.textContent, /HubSpot was changed\. 2 connections ended; their owners connect again\./);
    // Removing asks first, names what ends, and sends the revision shown.
    globalThis.confirm = message => { state.asked = message; return false; };
    await click(named(hubspot(), 'Remove HubSpot'));
    assert.match(state.asked, /Remove HubSpot from this workspace\? 2 connections of it end at once\./);
    assert.equal(writes.length, 1, 'declining the question removes nothing');
    globalThis.confirm = () => true;
    await click(named(hubspot(), 'Remove HubSpot'));
    assert.deepEqual(writes[1], { path: '/apps/added/hubspot/remove', body: { revision: 4 } });
    assert.match(document.body.textContent, /HubSpot was removed\./);
  }, { url, fetch: server(state, reads, writes) });
});

test('without an app library on the server there is nothing to add, and added apps can still be removed', async () => {
  const added = { revision: 1, toolkit: 'hubspot', added: { at: 1763283720, by_name: null }, actions: [{ tool: 'HUBSPOT_PING', title: 'Check the account', effect: 'read' }] };
  const state = { library: false, apps: [allowed('hubspot', 'HubSpot', { added })] };
  await reactFixture(async ({ render }) => {
    const { AllowedApps } = await import('../src/react/settings/allowed-apps.tsx');
    await render(AllowedApps);
    assert.equal(!!named(document, 'Add more apps'), false);
    assert.equal(!!named(document, 'Change the actions of HubSpot'), false);
    assert.ok(named(document, 'Remove HubSpot'));
  }, { url, fetch: server(state, [], []) });
});
