import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const workspace = '11111111-1111-4111-8111-111111111111';
const member = { id: 'employee', email: 'employee@example.com', role: 'Member' };
const owner = { id: 'owner', email: 'owner@example.com', role: 'Owner' };

async function fixture(viewerRole, run, respond) {
  const writes = [];
  const people = { role: viewerRole, members: [{ ...owner }, { ...member }], invitations: [] };
  await reactFixture(async context => {
    const { PeoplePage } = await import('../src/react/people.tsx');
    await context.render(PeoplePage);
    await run({ ...context, writes, people });
  }, { url: `http://localhost/w/${workspace}/?view=people`, fetch: async (path, options = {}) => {
    const body = options.body ? JSON.parse(options.body) : undefined;
    if (body !== undefined) {
      assert.equal(options.headers['X-Ayati-Request'], 'identity');
      assert.ok(path.startsWith(`/api/identity/workspaces/${workspace}/`)); writes.push({ path, body });
    }
    const custom = respond?.(path, body);
    if (custom) return { ok: custom.ok, json: async () => custom.data };
    if (path.endsWith('/people')) return { ok: true, json: async () => people };
    if (path.endsWith('/chat-directory')) return { ok: true, json: async () => ({ role: viewerRole, agents: [{ id: 'agent', name: '<script>Agent</script>' }], grants: [] }) };
    if (path.endsWith('/resource-access') && body === undefined) return { ok: true, json: async () => ({ apps: [{ id: 'app', name: 'Inventory' }], panels: [{ id: 'panel', active: { definition: { title: 'Stock' } } }], grants: [{ user: 'employee', kind: 'app', resource: 'app', access: 'read' }] }) };
    return { ok: true, json: async () => ({}) };
  } });
}

test('React invitations retain failed form values and require an explicit retry', async () => {
  let failing = true;
  await fixture('Owner', async ({ writes, button, click, change, submit }) => {
    assert.equal(document.querySelector('form'), null);
    await click(button('＋ Invite person'));
    const form = document.querySelector('form');
    assert.equal(document.activeElement, form.elements.email);
    await change(form.elements.email, 'invited@example.com'); await change(form.elements.role, 'Admin');
    assert.equal(writes.length, 0); await submit(form);
    assert.match(document.querySelector('dialog [role=status]').textContent, /Invitation failed/);
    assert.equal(form.elements.email.value, 'invited@example.com'); assert.equal(button('Send invitation').disabled, false);
    assert.equal(writes.length, 1); failing = false; await submit(form);
    assert.equal(document.querySelector('dialog'), null);
    assert.deepEqual(writes[1].body, { email: 'invited@example.com', role: 'Admin' });
  }, (path, body) => body && path.endsWith('/invitations') && failing ? { ok: false, data: { error: 'Invitation failed' } } : undefined);
});

test('opening a React member drawer grants nothing; rejected changes retain saved access', async () => {
  await fixture('Owner', async ({ writes, button, click, change }) => {
    await click(button(`View ${member.email}`));
    assert.equal(writes.length, 0); assert.equal(document.querySelector('script'), null);
    const agent = document.querySelector('dialog input[type=checkbox]'); await click(agent);
    assert.equal(agent.checked, false); assert.equal(agent.matches(':disabled'), false);
    assert.deepEqual(writes[0].body, { user: 'employee', agent: 'agent', allowed: true });
    const resources = [...document.querySelectorAll('dialog .ui-resource select')];
    assert.equal(resources[0].value, 'read'); await change(resources[0], 'write');
    assert.equal(resources[0].value, 'read');
    assert.deepEqual(writes[1].body, { user: 'employee', kind: 'app', resource: 'app', access: 'write' });
    assert.deepEqual([...resources[1].options].map(option => option.value), ['', 'read']);
    assert.equal(button('Close').disabled, false);
  }, (_path, body) => body ? { ok: false, data: { error: 'Access was revoked' } } : undefined);
});

test('React role and grant saves target only the selected person and resource', async () => {
  await fixture('Owner', async ({ writes, button, click, change }) => {
    await click(button(`View ${member.email}`));
    await change(document.querySelector('dialog .ui-field select'), 'Admin'); await click(button('Save role'));
    assert.deepEqual(writes[0].body, { role: 'Admin' }); assert.ok(writes[0].path.endsWith('/members/employee'));
    await change(document.querySelector('dialog .ui-resource select'), '');
    assert.deepEqual(writes[1].body, { user: 'employee', kind: 'app', resource: 'app', access: null });
    const agent = document.querySelector('dialog input[type=checkbox]'); await click(agent);
    assert.equal(agent.checked, true); assert.equal(writes.length, 3);
  });
});

test('React member view cannot edit roles, invite, remove, or grant access', async () => {
  await fixture('Member', async ({ writes, button, click }) => {
    assert.equal(button('＋ Invite person').hidden, true); await click(button(`View ${member.email}`));
    assert.equal(document.querySelector('dialog select').disabled, true);
    assert.equal(button('Save role'), undefined); assert.equal(button('Remove from workspace'), undefined);
    assert.equal(document.querySelectorAll('dialog [role=tab]').length, 1); assert.equal(writes.length, 0);
  });
});

test('React admins can invite members and remove members without owner grant controls', async () => {
  await fixture('Admin', async ({ button, click, writes }) => {
    await click(button('＋ Invite person'));
    assert.deepEqual([...document.querySelector('dialog select').options].map(option => option.value), ['Member']);
    await click(button('Close')); await click(button(`View ${member.email}`));
    assert.equal(button('Save role'), undefined); assert.ok(button('Remove from workspace'));
    assert.equal(document.querySelectorAll('dialog [role=tab]').length, 1); assert.equal(writes.length, 0);
  });
});

test('React owner membership remains immutable', async () => {
  await fixture('Owner', async ({ button, click }) => {
    await click(button(`View ${owner.email}`));
    assert.equal(document.querySelector('dialog select').disabled, true);
    assert.equal(button('Save role'), undefined); assert.equal(button('Remove from workspace'), undefined);
    assert.equal(document.querySelectorAll('dialog [role=tab]').length, 1);
  });
});

test('React tabs support keyboard selection and preserve the active tab during search', async () => {
  await fixture('Owner', async ({ dom, update, change }) => {
    const tabs = [...document.querySelectorAll('[role=tab]')];
    await update(() => tabs[0].dispatchEvent(new dom.window.KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true })));
    assert.equal(tabs[1].getAttribute('aria-selected'), 'true'); assert.equal(document.activeElement, tabs[1]);
    await change(document.querySelector('input[type=search]'), 'missing');
    assert.equal(tabs[1].getAttribute('aria-selected'), 'true');
    assert.equal(document.querySelector('[role=tabpanel]:not([hidden])').textContent, 'No invitations match your search.');
  });
});

test('React busy dialogs prevent duplicate sends and Escape dismissal', async () => {
  let release;
  const pending = new Promise(resolve => { release = resolve; });
  await fixture('Owner', async ({ dom, writes, button, click, change, submit, update }) => {
    await click(button('＋ Invite person')); const form = document.querySelector('form');
    await change(form.elements.email, 'invited@example.com'); await submit(form); await submit(form);
    assert.equal(writes.length, 1); assert.equal(button('Close').disabled, true);
    assert.equal(form.elements.email.matches(':disabled'), true);
    const cancel = new dom.window.Event('cancel', { cancelable: true });
    await update(() => document.querySelector('dialog').dispatchEvent(cancel)); assert.equal(cancel.defaultPrevented, true);
    assert.ok(document.querySelector('dialog'));
    await update(() => release({ error: 'Try again' }));
    assert.equal(button('Close').disabled, false); assert.equal(button('Send invitation').disabled, false);
  }, (path, body) => body && path.endsWith('/invitations') ? { ok: false, data: pending } : undefined);
});

test('React safely ignores a late member-access read after closing the drawer', async () => {
  let release;
  const pending = new Promise(resolve => { release = resolve; });
  await fixture('Owner', async ({ button, click, update, writes }) => {
    await click(button(`View ${member.email}`)); await click(button('Close'));
    assert.equal(document.activeElement?.getAttribute('aria-label'), `View ${member.email}`);
    await update(() => release({ role: 'Owner', agents: [], grants: [] }));
    assert.equal(document.querySelector('dialog'), null); assert.equal(writes.length, 0);
  }, path => path.endsWith('/chat-directory') ? { ok: true, data: pending } : undefined);
});
