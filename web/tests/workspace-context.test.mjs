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
    globalThis.fetch = async () => ({ ok: true, status: 200, json: async () => ({ user: { id: user, email: 'test@example.com' }, workspaces: [{ id: first, name: 'First', role: 'Admin' }, { id: second, name: 'Second', role: 'Member' }] }) });
    globalThis.location = { pathname: `/w/${first}/`, replace: () => assert.fail('unexpected redirect') };
    const a = await import('../src/workspace-context.ts?first');
    assert.equal(await a.enterWorkspace(), true);
    assert.equal(a.workspaceUrl('/api/events?agent=a'), `/w/${first}/api/events?agent=a`);
    assert.equal(a.currentWorkspace()?.role, 'Admin');
    const key = a.storageScope('draft');
    assert.equal(saved.get('relay.last-workspace.one'), first);
    globalThis.location = { pathname: `/w/${second}/`, replace: () => assert.fail('unexpected redirect') };
    const b = await import('../src/workspace-context.ts?second');
    await b.enterWorkspace();
    assert.notEqual(b.storageScope('draft'), key);
    assert.equal(b.currentWorkspace()?.role, 'Member');
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

test('accounts without a display name finish setup before mounting a workspace', async () => {
  const original = { location: globalThis.location, fetch: globalThis.fetch };
  try {
    let target;
    globalThis.location = { pathname: `/w/${first}/`, replace: value => { target = value; } };
    globalThis.fetch = async () => ({ ok: true, status: 200, json: async () => ({ user: { id: 'owner', display_name: null }, workspaces: [{ id: first, name: 'Business', role: 'Admin' }] }) });
    const context = await import('../src/workspace-context.ts?name-setup');
    assert.equal(await context.enterWorkspace(), false);
    assert.equal(target, '/?view=account&choose=1');
  } finally { Object.assign(globalThis, original); }
});
