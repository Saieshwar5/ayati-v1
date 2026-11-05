import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

const w = '11111111-1111-4111-8111-111111111111';
const base = `http://localhost/w/${w}/?view=apps`;
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const session = role => response({ user: { id: 'me', email: 'me@test' }, workspaces: [{ id: w, name: 'Mehta', role }] });
const zoho = { id: 'c1', server: 'zoho', name: 'Zoho Books', kind: 'service', provider: 'composio', account: 'accounts@mehta.test', status: 'connected', rules: 2,
  actions: [{ id: 'invoice.read', title: 'Read invoices', meaning: 'read', mode: 'allow' }, { id: 'invoice.create', title: 'Create an invoice', meaning: 'money', mode: 'ask' }, { id: 'invoice.delete', title: 'Delete an invoice', meaning: 'delete', mode: 'block' }],
  agents: [{ id: 'a1', name: 'Atlas', avatar: 'robot-01' }, { id: 'a2', name: 'Ledger', avatar: 'robot-05' }] };
const gmail = { id: 'c2', server: 'gmail', name: 'Gmail', kind: 'service', provider: 'composio', account: null, status: 'reconnect', rules: 0, actions: [{ id: 'mail.send', title: 'Send email', meaning: 'sends_outside', mode: 'ask' }], agents: [] };

test('Members see every connected app, what agents may do and who uses it, read-only', async () => {
  await reactFixture(async ({ render, click }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { AppsPage } = await import('../src/react/apps/apps-page.tsx');
    await render(AppsPage, { href: base });
    assert.deepEqual([...document.querySelectorAll('.app-name')].map(n => n.textContent), ['Zoho Books', 'Gmail']);
    const panel = document.querySelector('.app-panel');
    assert.match(panel.textContent, /Zoho Books.*Read invoices.*Reads.*Allow.*Create an invoice.*Money.*Ask first.*Delete an invoice.*Deletes.*Block/);
    assert.match(panel.textContent, /Used by.*Atlas, Ledger/);
    assert.match(panel.textContent, /2 rules cover this app/);
    assert.equal([...document.querySelectorAll('button')].some(b => ['Add app', 'Manage', 'Reconnect'].includes(b.textContent)), false);
    // The "need you" pill shows only apps with a problem.
    await click(document.querySelector('.apps-needs'));
    assert.deepEqual([...document.querySelectorAll('.app-name')].map(n => n.textContent), ['Gmail']);
    assert.match(document.querySelector('.app-panel').textContent, /until an admin reconnects it/);
  }, { url: base, fetch: async path => path.endsWith('/session') ? session('Member') : response({ apps: [{ ...zoho, account: null }, gmail], can_manage: false }) });
});

test('Admins add and manage apps from Apps, and old Connections links open it there', async () => {
  await reactFixture(async ({ render, click }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { AppsPage } = await import('../src/react/apps/apps-page.tsx');
    await render(AppsPage, { href: `http://localhost/w/${w}/?view=settings&section=connections` });
    assert.ok(document.querySelector('dialog[open]')?.textContent.includes('Add and manage apps'), 'an old link opens the manage drawer');
    await click([...document.querySelectorAll('button')].find(b => b.getAttribute('aria-label') === 'Close'));
    assert.match(document.querySelector('.app-line').textContent, /accounts@mehta\.test/);
    await click([...document.querySelectorAll('button')].find(b => b.textContent === 'Add app'));
    assert.ok(document.querySelector('dialog[open] [aria-label="Workspace connections"]'));
  }, { url: `http://localhost/w/${w}/?view=apps`, fetch: async path => path.endsWith('/session') ? session('Admin')
    : path.endsWith('/control/connections') ? response({ servers: [], systems: [], connections: [], safety: [], people: [], viewer: 'me', execution_available: true })
    : response({ apps: [zoho], can_manage: true }) });
});
