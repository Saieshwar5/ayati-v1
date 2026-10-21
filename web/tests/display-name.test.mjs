import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const user = { id: 'owner', email: 'owner@example.com', display_name: null };
const workspace = { id: '11111111-1111-4111-8111-111111111111', name: 'Store', role: 'Owner' };

test('first setup asks for a name without writing or automatically entering a workspace', async () => {
  const writes = [];
  await reactFixture(async ({ render, change, submit, navigation, button }) => {
    const { AccountPage } = await import('../src/react/account.tsx');
    await render(AccountPage);
    assert.match(document.body.textContent, /What should people call you/);
    assert.deepEqual(navigation, []); assert.deepEqual(writes, []);
    const form = document.querySelector('form');
    await change(form.elements.display_name, 'Sai Eshwar');
    await submit(form);
    assert.deepEqual(writes, [{ display_name: 'Sai Eshwar' }]);
    assert.ok(button('Store · Owner'));
    assert.equal(document.body.textContent.includes('What should people call you'), false);
  }, { fetch: async (path, options = {}) => {
    if (options.method !== 'POST') return { ok: true, json: async () => ({ user, workspaces: [workspace] }) };
    assert.equal(path, '/api/identity/profile');
    assert.equal(options.headers['X-Ayati-Request'], 'identity');
    writes.push(JSON.parse(options.body));
    return { ok: true, json: async () => ({ ...user, display_name: 'Sai Eshwar' }) };
  } });
});

test('failed name saves retain drafts; pending saves suppress duplicates and ignore unmounted responses', async () => {
  let finish;
  let calls = 0;
  const saved = [];
  await reactFixture(async ({ render, change, submit, update }) => {
    const { DisplayNameForm } = await import('../src/react/display-name-form.tsx');
    await render(DisplayNameForm, { user: { ...user, display_name: 'Old name' }, saved: value => saved.push(value) });
    const form = document.querySelector('form');
    await change(form.elements.display_name, 'New name');
    await submit(form);
    assert.match(document.querySelector('[role=status]').textContent, /Try again/);
    assert.equal(form.elements.display_name.value, 'New name');
    await submit(form); await submit(form);
    assert.equal(calls, 2);
    assert.equal(form.querySelector('fieldset').disabled, true);
    await render(() => null);
    await update(() => finish({ ok: true, json: async () => ({ ...user, display_name: 'New name' }) }));
    assert.deepEqual(saved, []);
  }, { fetch: async () => {
    calls++;
    if (calls === 1) return { ok: false, json: async () => ({ error: 'Try again' }) };
    return new Promise(resolve => { finish = resolve; });
  } });
});

test('Users searches display names while keeping email available for identification', async () => {
  await reactFixture(async ({ render, change }) => {
    const { PeoplePage } = await import('../src/react/people.tsx');
    await render(PeoplePage);
    assert.equal(document.querySelector('.ui-row-title strong').textContent, 'Sai Eshwar');
    const search = document.querySelector('input[type=search]');
    await change(search, 'sai'); assert.equal(document.querySelectorAll('.ui-row').length, 1);
    await change(search, 'owner@example.com'); assert.equal(document.querySelectorAll('.ui-row').length, 1);
    await change(search, 'missing'); assert.equal(document.querySelectorAll('.ui-row').length, 0);
  }, { fetch: async () => ({ ok: true, json: async () => ({ role: 'Member', members: [{ ...user, display_name: 'Sai Eshwar', role: 'Member' }], invitations: [] }) }) });
});

test('expired profile sessions return to sign-in without a successful save callback', async () => {
  const saved = [];
  await reactFixture(async ({ render, change, submit, navigation }) => {
    const { DisplayNameForm } = await import('../src/react/display-name-form.tsx');
    await render(DisplayNameForm, { user, saved: value => saved.push(value) });
    const form = document.querySelector('form');
    await change(form.elements.display_name, 'Sai'); await submit(form);
    assert.deepEqual(navigation, ['/?view=account']); assert.deepEqual(saved, []);
  }, { fetch: async () => ({ ok: false, status: 401, json: async () => ({ error: 'Session ended' }) }) });
});
