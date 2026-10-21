import { test } from 'node:test';
import assert from 'node:assert/strict';

const first = '11111111-1111-4111-8111-111111111111';
const second = '22222222-2222-4222-8222-222222222222';

test('workspace URLs and drafts stay scoped to both the account and workspace', async () => {
  const original = { location: globalThis.location, fetch: globalThis.fetch, localStorage: globalThis.localStorage, setInterval: globalThis.setInterval };
  try {
    let user = 'one';
    const saved = new Map();
    globalThis.localStorage = { setItem: (key, value) => saved.set(key, value) };
    globalThis.setInterval = () => 0;
    globalThis.fetch = async () => ({ ok: true, status: 200, json: async () => ({ user: { id: user, email: 'test@example.com' }, workspaces: [{ id: first, name: 'First', role: 'Owner' }, { id: second, name: 'Second', role: 'Member' }] }) });
    globalThis.location = { pathname: `/w/${first}/`, replace: () => assert.fail('unexpected redirect') };
    const a = await import('../src/workspace-context.ts?first');
    assert.equal(await a.enterWorkspace(), true);
    assert.equal(a.workspaceUrl('/api/events?agent=a'), `/w/${first}/api/events?agent=a`);
    assert.equal(a.canManageResources(), true);
    const key = a.storageScope('draft');
    assert.equal(saved.get('relay.last-workspace.one'), first);
    globalThis.location = { pathname: `/w/${second}/`, replace: () => assert.fail('unexpected redirect') };
    const b = await import('../src/workspace-context.ts?second');
    await b.enterWorkspace();
    assert.notEqual(b.storageScope('draft'), key);
    assert.equal(b.canManageResources(), false);
    user = 'two';
    const c = await import('../src/workspace-context.ts?other-user');
    await c.enterWorkspace();
    assert.notEqual(c.storageScope('draft'), b.storageScope('draft'));
  } finally { Object.assign(globalThis, original); }
});

test('missing membership and expired sessions redirect before the client mounts', async () => {
  const original = { location: globalThis.location, fetch: globalThis.fetch };
  try {
    let target;
    globalThis.location = { pathname: `/w/${first}/`, replace: value => { target = value; } };
    globalThis.fetch = async () => ({ ok: true, status: 200, json: async () => ({ user: { id: 'other' }, workspaces: [] }) });
    const absent = await import('../src/workspace-context.ts?missing-membership');
    assert.equal(await absent.enterWorkspace(), false);
    assert.equal(target, '/?view=account&choose=1');
    target = undefined;
    globalThis.fetch = async () => ({ ok: false, status: 401 });
    const expired = await import('../src/workspace-context.ts?expired');
    assert.equal(await expired.enterWorkspace(), false);
    assert.equal(target, '/?view=account&choose=1');
  } finally { Object.assign(globalThis, original); }
});

test('one authenticated startup request migrates layouts only for the adopted owner', async () => {
  const original = { location: globalThis.location, fetch: globalThis.fetch, localStorage: globalThis.localStorage, setInterval: globalThis.setInterval };
  try {
    const saved = new Map([['ayati.home.layout', 'original-layout']]);
    globalThis.localStorage = { setItem: (k, v) => saved.set(k, v), getItem: k => saved.get(k) ?? null };
    globalThis.setInterval = () => 0;
    globalThis.location = { pathname: `/w/${first}/`, replace: () => assert.fail('unexpected redirect') };
    let adopted = true;
    let role = 'Owner';
    const calls = [];
    globalThis.fetch = async path => {
      calls.push(path);
      assert.equal(path, '/api/identity/session');
      return { ok: true, status: 200, json: async () => ({ user: { id: 'owner' }, workspaces: [{ id: first, name: 'First', role, adopted }] }) };
    };
    const owner = await import('../src/workspace-context.ts?adopted-owner');
    assert.equal(await owner.enterWorkspace(), true);
    assert.deepEqual(calls, ['/api/identity/session']);
    assert.equal(saved.get(owner.storageScope('ayati.home.layout')), 'original-layout');
    saved.set(owner.storageScope('ayati.home.layout'), 'new-layout');
    await owner.enterWorkspace();
    assert.equal(saved.get(owner.storageScope('ayati.home.layout')), 'new-layout');
    saved.delete(owner.storageScope('ayati.home.layout'));
    adopted = false;
    await owner.enterWorkspace();
    assert.equal(saved.has(owner.storageScope('ayati.home.layout')), false);
    adopted = true; role = 'Member';
    await owner.enterWorkspace();
    assert.equal(saved.has(owner.storageScope('ayati.home.layout')), false);
  } finally { Object.assign(globalThis, original); }
});

test('accounts without a display name finish setup before mounting a workspace', async () => {
  const original = { location: globalThis.location, fetch: globalThis.fetch };
  try {
    let target;
    globalThis.location = { pathname: `/w/${first}/`, replace: value => { target = value; } };
    globalThis.fetch = async () => ({ ok: true, status: 200, json: async () => ({ user: { id: 'owner', display_name: null }, workspaces: [{ id: first, name: 'Business', role: 'Owner' }] }) });
    const context = await import('../src/workspace-context.ts?name-setup');
    assert.equal(await context.enterWorkspace(), false);
    assert.equal(target, '/?view=account&choose=1');
  } finally { Object.assign(globalThis, original); }
});
