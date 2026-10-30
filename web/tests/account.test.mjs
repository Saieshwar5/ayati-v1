import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const first = '11111111-1111-4111-8111-111111111111';
const second = '22222222-2222-4222-8222-222222222222';
const session = { user: { id: 'owner', email: 'owner@example.com', display_name: 'Owner Person' }, workspaces: [{ id: first, name: 'Store', role: 'Owner', people: 3, agents: 1 }] };
const ok = data => ({ ok: true, status: 200, json: async () => data });
const fail = (error, status = 400) => ({ ok: false, status, json: async () => ({ error }) });

test('invitations are explicit: accept opens the workspace, decline closes it and reloads', async () => {
  const writes = [];
  await reactFixture(async ({ render, navigation, button, click }) => {
    const { AccountPage } = await import('../src/react/account.tsx'); await render(AccountPage);
    assert.deepEqual(navigation, []); assert.deepEqual(writes, []);
    assert.match(document.body.textContent, /Meera invited you to Studio/);
    assert.match(document.body.textContent, /Owner · 3 people · 1 agent/);
    await click(button('Decline'));
    assert.deepEqual(writes, [{ path: '/api/identity/invitations/invite/decline', body: {} }]);
    await click(button('Accept'));
    assert.deepEqual(writes[1], { path: '/api/identity/invitations/invite/accept', body: {} });
    assert.deepEqual(navigation, [`/w/${second}/`]);
  }, { url: 'http://localhost/?view=account', fetch: async (path, options = {}) => {
    if (options.method === 'POST') {
      assert.equal(options.headers['X-Ayati-Request'], 'identity'); writes.push({ path, body: JSON.parse(options.body) });
      return ok(path.endsWith('/decline') ? { declined: true } : { id: second });
    }
    return ok({ ...session, invitations: [{ id: 'invite', name: 'Studio', role: 'Member', inviter: 'Meera', expires: Date.now() / 1000 + 86400 * 6 }] });
  } });
});

test('without admin rights a workspace needs an invite code; failures keep the name, code and request ID', async () => {
  const writes = [];
  let failing = true;
  await reactFixture(async ({ render, button, click, change, submit, navigation }) => {
    const { AccountPage } = await import('../src/react/account.tsx'); await render(AccountPage);
    assert.equal(button('Create a workspace'), undefined, 'no open create button');
    await click(button('Enter an invite code'));
    const codeForm = document.querySelector('form'); await change(codeForm.elements.code, 'cko-aaaa-bbbb-cccc');
    await submit(codeForm);
    assert.deepEqual(writes[0], { path: '/api/identity/workspace-codes/check', body: { code: 'cko-aaaa-bbbb-cccc' } });
    assert.match(document.body.textContent, /Invite code accepted · valid for 9 more days/);
    const form = document.querySelector('form'); await change(form.elements.name, 'My business');
    await submit(form); assert.match(document.querySelector('[role=alert]').textContent, /Try again/);
    assert.equal(form.elements.name.value, 'My business');
    failing = false; await submit(form);
    assert.deepEqual(writes[1], writes[2]);
    assert.equal(writes[1].body.code, 'cko-aaaa-bbbb-cccc'); assert.equal(writes[1].body.name, 'My business');
    assert.match(writes[1].body.request_id, /^[0-9a-f-]{36}$/); assert.deepEqual(navigation, [`/w/${second}/`]);
  }, { url: 'http://localhost/?view=account&choose=1', fetch: async (path, options = {}) => {
    if (options.method !== 'POST') return ok(session);
    writes.push({ path, body: JSON.parse(options.body) });
    if (path.endsWith('/check')) return ok({ expires_at: Date.now() / 1000 + 86400 * 8.5 });
    assert.equal(path, '/api/identity/workspaces');
    return failing ? fail('Try again') : ok({ id: second });
  } });
});

test('a person with no workspace and no invitation lands on the invite-code page; a wrong code shows the error', async () => {
  await reactFixture(async ({ render, change, submit, navigation }) => {
    const { AccountPage } = await import('../src/react/account.tsx'); await render(AccountPage);
    assert.deepEqual(navigation, []); assert.equal(document.body.textContent.includes('Studio'), false);
    assert.match(document.body.textContent, /not in a workspace yet/);
    assert.match(document.body.textContent, /invite owner@example\.com/);
    const form = document.querySelector('form'); await change(form.elements.code, 'CKO-ZZZZ-ZZZZ-ZZZZ'); await submit(form);
    assert.match(document.querySelector('[role=alert]').textContent, /Code invalid or expired/);
  }, { fetch: async (path, options = {}) => options.method === 'POST' ? fail('Code invalid or expired') : ok({ user: session.user, workspaces: [] }) });
});

test('platform administrators create without a code and manage codes; a new code is shown once', async () => {
  const writes = [];
  let codes = [];
  await reactFixture(async ({ render, button, click, change, submit }) => {
    const { AccountPage } = await import('../src/react/account.tsx'); await render(AccountPage);
    assert.ok(button('Create a workspace')); assert.ok([...document.querySelectorAll('a')].some(a => a.getAttribute('href') === '/?view=account&admin=1'));
    await click(button('Create a workspace')); assert.match(document.body.textContent, /Cuckoo administrator/);
  }, { url: 'http://localhost/?view=account&choose=1', fetch: async () => ok({ ...session, platform_admin: true }) });

  await reactFixture(async ({ render, change, submit, until, update }) => {
    const { AccountPage } = await import('../src/react/account.tsx'); await render(AccountPage);
    await update(async () => { await new Promise(resolve => setTimeout(resolve, 0)); });
    assert.match(document.body.textContent, /No codes yet/);
    const form = document.querySelector('form');
    await change(form.querySelector('input'), 'Mehta Furnishings'); await submit(form);
    await update(async () => { await new Promise(resolve => setTimeout(resolve, 0)); });
    assert.deepEqual(writes[0], { path: '/api/identity/admin/workspace-codes', body: { note: 'Mehta Furnishings', days: 14 } });
    assert.equal(document.querySelector('.access-issued code').textContent, 'CKO-ABCD-EFGH-JKMN');
    assert.match(document.body.textContent, /not shown again/);
    assert.match(document.querySelector('[role=table]').textContent, /Mehta Furnishings.*Unused/);
  }, { url: 'http://localhost/?view=account&admin=1', fetch: async (path, options = {}) => {
    if (path === '/api/identity/session') return ok({ ...session, platform_admin: true });
    if (options.method === 'POST') {
      writes.push({ path, body: JSON.parse(options.body) });
      codes = [{ id: 'c1', note: 'Mehta Furnishings', created_at: 1, expires_at: Date.now() / 1000 + 86400 * 14, status: 'unused' }];
      return ok({ id: 'c1', code: 'CKO-ABCD-EFGH-JKMN', note: 'Mehta Furnishings', expires_at: 0 });
    }
    return ok({ codes });
  } });
});

test('sign-in continues to WorkOS for Google or email and never asks for a password', async () => {
  await reactFixture(async ({ render, navigation }) => {
    const { AccountPage } = await import('../src/react/account.tsx'); await render(AccountPage);
    assert.equal(document.querySelector('a.access-button').getAttribute('href'), '/auth/login?method=google');
    const form = document.querySelector('form');
    assert.equal(form.getAttribute('action'), '/auth/login'); assert.equal(form.elements.email.type, 'email');
    assert.equal(document.querySelector('input[type=password]'), null); assert.deepEqual(navigation, []);
    assert.match(document.body.textContent, /invite-only during early access/);
  }, { fetch: async (_path, options = {}) => {
    assert.notEqual(options.method, 'POST'); return ok({ user: null });
  } });
});

test('a remembered workspace that is no longer allowed is not reopened', async () => {
  await reactFixture(async ({ render, navigation }) => {
    localStorage.setItem('relay.last-workspace.owner', second);
    const { AccountPage } = await import('../src/react/account.tsx'); await render(AccountPage);
    assert.deepEqual(navigation, []); assert.equal(document.body.textContent.includes('Studio'), false);
  }, { fetch: async () => ok({ user: session.user, workspaces: [] }) });
});
