import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const first = '11111111-1111-4111-8111-111111111111';
const second = '22222222-2222-4222-8222-222222222222';
const session = { user: { id: 'owner', email: 'owner@example.com', display_name: 'Owner' }, workspaces: [{ id: first, name: 'Store', role: 'Owner' }] };

test('React workspace selection is explicit when invitations or choose are present', async () => {
  const writes = [];
  await reactFixture(async ({ render, navigation, button, click }) => {
    const { AccountPage } = await import('../src/react/account.tsx'); await render(AccountPage);
    assert.deepEqual(navigation, []); assert.deepEqual(writes, []);
    assert.match(document.body.textContent, /Invitation to Studio/);
    await click(button('Accept invitation'));
    assert.deepEqual(writes, [{ path: '/api/identity/invitations/invite/accept', body: {} }]);
    assert.deepEqual(navigation, [`/w/${second}/`]);
  }, { url: 'http://localhost/?view=account', fetch: async (path, options = {}) => {
    if (options.method === 'POST') {
      assert.equal(options.headers['X-Ayati-Request'], 'identity'); writes.push({ path, body: JSON.parse(options.body) });
      return { ok: true, json: async () => ({ id: second }) };
    }
    return { ok: true, json: async () => ({ ...session, invitations: [{ id: 'invite', name: 'Studio', role: 'Member' }] }) };
  } });
});

test('React workspace creation preserves name and idempotency key across a failed request', async () => {
  const writes = [];
  let failing = true;
  await reactFixture(async ({ render, button, click, change, submit, navigation }) => {
    const { AccountPage } = await import('../src/react/account.tsx'); await render(AccountPage);
    assert.deepEqual(navigation, []); await click(button('Create a workspace'));
    const form = document.querySelector('form'); await change(form.elements.name, 'My business');
    await submit(form); assert.match(document.querySelector('[role=alert]').textContent, /Try again/);
    assert.equal(form.elements.name.value, 'My business'); assert.equal(writes.length, 1);
    failing = false; await submit(form);
    assert.deepEqual(writes[0], writes[1]); assert.equal(writes[0].body.name, 'My business');
    assert.match(writes[0].body.request_id, /^[0-9a-f-]{36}$/); assert.deepEqual(navigation, [`/w/${second}/`]);
  }, { url: 'http://localhost/?view=account&choose=1', fetch: async (path, options = {}) => {
    if (options.method !== 'POST') return { ok: true, json: async () => session };
    assert.equal(path, '/api/identity/workspaces'); assert.equal(options.headers['X-Ayati-Request'], 'identity');
    writes.push({ path, body: JSON.parse(options.body) });
    return { ok: !failing, json: async () => failing ? { error: 'Try again' } : { id: second } };
  } });
});

test('React sign-in keeps authentication on the hosted login route without accepting invitations', async () => {
  await reactFixture(async ({ render, navigation }) => {
    const { AccountPage } = await import('../src/react/account.tsx'); await render(AccountPage);
    assert.equal(document.querySelector('a').getAttribute('href'), '/auth/login');
    assert.equal(document.querySelector('input[type=password]'), null); assert.deepEqual(navigation, []);
  }, { fetch: async (_path, options = {}) => {
    assert.notEqual(options.method, 'POST'); return { ok: true, json: async () => ({ user: null }) };
  } });
});

test('React does not restore a remembered workspace that is no longer allowed', async () => {
  await reactFixture(async ({ render, navigation }) => {
    localStorage.setItem('relay.last-workspace.owner', second);
    const { AccountPage } = await import('../src/react/account.tsx'); await render(AccountPage);
    assert.deepEqual(navigation, []); assert.equal(document.body.textContent.includes('Studio'), false);
  }, { fetch: async () => ({ ok: true, json: async () => ({ user: session.user, workspaces: [] }) }) });
});
