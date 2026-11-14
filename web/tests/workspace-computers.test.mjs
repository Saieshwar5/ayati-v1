import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Settings → Computers → Your computers (B3): machines the workspace runs itself.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=settings&section=computers`;
const response = data => ({ ok: true, json: async () => data });
const field = label => [...document.querySelectorAll('label')].find(node => node.textContent.startsWith(label)).querySelector('input');
const check = label => [...document.querySelectorAll('dialog label')].find(node => node.textContent === label).querySelector('input');
const office = { id: 'c1', name: 'Office PC', state: 'online', platform: 'Windows 11', revision: 3, busy: false, agents: [{ id: 'atlas', name: 'Atlas' }] };

test('an admin adds a computer once, sees its one-time code and chooses its agents', async () => {
  const writes = [];
  let computers = [];
  let resolveAdd;
  await reactFixture(async ({ render, button, click, change, update }) => {
    const { WorkspaceComputers } = await import('../src/react/settings/workspace-computers.tsx');
    await render(WorkspaceComputers);
    assert.match(document.body.textContent, /No computers added yet/);
    assert.match(document.body.textContent, /permissions of the account that runs the connector/);
    await change(field('Computer name'), '  Office PC ');
    // Two quick clicks send one request.
    const adding = button('Add computer');
    await update(() => { adding.click(); adding.click(); });
    assert.equal(writes.filter(entry => entry.path.endsWith('/workspace-computers')).length, 1);
    computers = [{ ...office, agents: [] }];
    await update(resolveAdd);
    const dialog = document.querySelector('dialog');
    assert.match(dialog.textContent, /--computer c1 --code ABCD-EFGH-JKLM-NPQR/);
    assert.match(dialog.textContent, /works once, for 15 minutes/);
    await click(button('Done'));
    assert.equal(document.querySelector('dialog'), null);
    assert.match(document.body.textContent, /Office PCOnline · Windows 11No agents use it yet/);

    await click(button('Choose agents'));
    await click(check('Atlas'));
    computers = [office];
    await click(button('Save agents'));
    const saved = writes.find(entry => entry.path.endsWith('/c1/agents'));
    assert.deepEqual(saved.body, { revision: 3, agents: ['atlas'] });
    assert.match(document.body.textContent, /Used by Atlas/);
  }, { url, fetch: async (path, options = {}) => {
    const body = options.body ? JSON.parse(options.body) : undefined;
    if (body) writes.push({ path, body });
    if (path.endsWith('/agent-profiles')) return response({ agents: [{ id: 'atlas', name: 'Atlas' }, { id: 'ledger', name: 'Ledger' }] });
    if (body && path.endsWith('/workspace-computers')) {
      assert.equal(body.name, 'Office PC');
      assert.match(body.id, /^[0-9a-f-]{36}$/);
      return new Promise(resolve => { resolveAdd = () => resolve(response({ id: 'c1', code: 'ABCD-EFGH-JKLM-NPQR', expires_minutes: 15 })); });
    }
    if (body) return response({});
    return response({ computers, can_change: true });
  } });
});

test('removing asks first, and a failed removal keeps the computer listed', async () => {
  await reactFixture(async ({ render, button, click }) => {
    const { WorkspaceComputers } = await import('../src/react/settings/workspace-computers.tsx');
    await render(WorkspaceComputers);
    await click(button('Remove'));
    assert.match(document.querySelector('dialog').textContent, /signed out/);
    await click(button('Remove computer'));
    assert.match(document.body.textContent, /This computer changed; reload before removing it/);
    assert.match(document.body.textContent, /Office PC/);
  }, { url, fetch: async (path, options = {}) => {
    if (options.body) return { ok: false, status: 409, json: async () => ({ error: 'This computer changed; reload before removing it' }) };
    return response({ computers: [office], can_change: true });
  } });
});

test('members see the computers without changing them', async () => {
  await reactFixture(async ({ render }) => {
    const { WorkspaceComputers } = await import('../src/react/settings/workspace-computers.tsx');
    await render(WorkspaceComputers);
    assert.match(document.body.textContent, /Used by Atlas/);
    assert.equal([...document.querySelectorAll('button')].some(node => ['Remove', 'Add computer', 'New code'].includes(node.textContent)), false);
    assert.match(document.body.textContent, /Only admins add and change these/);
  }, { url, fetch: async () => response({ computers: [{ ...office, state: 'offline' }], can_change: false }) });
});
