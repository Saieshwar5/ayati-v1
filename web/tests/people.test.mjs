import { test } from 'node:test';
import assert from 'node:assert/strict';
import { JSDOM } from 'jsdom';

const workspace = '11111111-1111-4111-8111-111111111111';
const member = { id: 'employee', email: 'employee@example.com', role: 'Member' };
const owner = { id: 'owner', email: 'owner@example.com', role: 'Owner' };
const settle = async () => { for (let i = 0; i < 4; i++) await new Promise(resolve => setImmediate(resolve)); };

async function fixture(viewerRole, run, respond) {
  const keys = ['document', 'location', 'Option', 'fetch', 'confirm'];
  const previous = Object.fromEntries(keys.map(k => [k, globalThis[k]]));
  const dom = new JSDOM('<div id="app"></div>', { url: `http://localhost/w/${workspace}/?view=people` });
  const writes = [];
  const people = { role: viewerRole, members: [{ ...owner }, { ...member }], invitations: [] };
  try {
    globalThis.document = dom.window.document; globalThis.Option = dom.window.Option;
    globalThis.location = dom.window.location; globalThis.confirm = () => true;
    dom.window.HTMLDialogElement.prototype.showModal = function () { this.open = true; };
    dom.window.HTMLDialogElement.prototype.close = function () { this.open = false; this.dispatchEvent(new dom.window.Event('close')); };
    globalThis.fetch = async (path, options = {}) => {
      const body = options.body ? JSON.parse(options.body) : undefined;
      if (body !== undefined) {
        assert.equal(options.headers['X-Ayati-Request'], 'identity');
        assert.ok(path.startsWith(`/api/identity/workspaces/${workspace}/`));
        writes.push({ path, body });
      }
      const custom = respond?.(path, body);
      if (custom) return { ok: custom.ok, json: async () => custom.data };
      if (path.endsWith('/people')) return { ok: true, json: async () => people };
      if (path.endsWith('/chat-directory')) return { ok: true, json: async () => ({ role: viewerRole, users: [owner, member], agents: [{ id: 'agent', name: '<script>Agent</script>' }], grants: [] }) };
      if (path.endsWith('/resource-access') && body === undefined) return { ok: true, json: async () => ({ apps: [{ id: 'app', name: 'Inventory', active_revision: 1 }], panels: [{ id: 'panel', active: { definition: { title: 'Stock' } } }], grants: [{ user: 'employee', kind: 'app', resource: 'app', access: 'read' }] }) };
      return { ok: true, json: async () => ({}) };
    };
    const { installPeople } = await import('../src/people.ts');
    await installPeople();
    const button = label => [...document.querySelectorAll('button')].find(b => b.textContent === label || b.getAttribute('aria-label') === label);
    await run({ dom, writes, people, button });
  } finally { Object.assign(globalThis, previous); dom.window.close(); }
}

test('Users starts without forms; invitation errors retain fields and never silently retry', async () => {
  let failing = true;
  await fixture('Owner', async ({ dom, writes, button }) => {
    assert.equal(document.querySelector('form'), null);
    button('＋ Invite person').click();
    const form = document.querySelector('form');
    form.elements.email.value = 'invited@example.com'; form.elements.role.value = 'Admin';
    assert.equal(writes.length, 0);
    await form.onsubmit(new dom.window.Event('submit'));
    assert.match(document.querySelector('dialog [role=status]').textContent, /Invitation failed/);
    assert.equal(form.elements.email.value, 'invited@example.com');
    assert.equal(button('Send invitation').disabled, false);
    assert.equal(writes.length, 1);
    failing = false;
    await form.onsubmit(new dom.window.Event('submit'));
    assert.equal(document.querySelector('dialog'), null);
    assert.deepEqual(writes[1].body, { email: 'invited@example.com', role: 'Admin' });
  }, (path, body) => body && path.endsWith('/invitations') && failing ? { ok: false, data: { error: 'Invitation failed' } } : undefined);
});

test('member details do not grant access on open; failed edits restore saved grants', async () => {
  await fixture('Owner', async ({ dom, writes, button }) => {
    button(`View ${member.email}`).click(); await settle();
    assert.equal(writes.length, 0); assert.equal(document.querySelector('script'), null);
    const agent = document.querySelector('dialog input[type=checkbox]');
    agent.checked = true; await agent.onchange(new dom.window.Event('change'));
    assert.equal(agent.checked, false); assert.equal(agent.disabled, false);
    assert.deepEqual(writes[0].body, { user: 'employee', agent: 'agent', allowed: true });
    const resources = [...document.querySelectorAll('dialog .ui-resource select')];
    assert.equal(resources[0].value, 'read');
    resources[0].value = 'write'; await resources[0].onchange(new dom.window.Event('change'));
    assert.equal(resources[0].value, 'read');
    assert.deepEqual(writes[1].body, { user: 'employee', kind: 'app', resource: 'app', access: 'write' });
    assert.deepEqual([...resources[1].options].map(o => o.value), ['', 'read']);
    assert.equal(document.querySelector('dialog [aria-label=Close]').disabled, false);
  }, (_path, body) => body ? { ok: false, data: { error: 'Access was revoked' } } : undefined);
});

test('successful explicit access changes target only the selected person and resource', async () => {
  await fixture('Owner', async ({ dom, writes, button }) => {
    button(`View ${member.email}`).click(); await settle();
    const role = document.querySelector('dialog .ui-field select');
    role.value = 'Admin'; button('Save role').click(); await settle();
    assert.deepEqual(writes[0].body, { role: 'Admin' });
    assert.ok(writes[0].path.endsWith('/members/employee'));
    const app = document.querySelector('dialog .ui-resource select');
    app.value = ''; await app.onchange(new dom.window.Event('change'));
    assert.deepEqual(writes[1].body, { user: 'employee', kind: 'app', resource: 'app', access: null });
    const agent = document.querySelector('dialog input[type=checkbox]');
    agent.checked = true; await agent.onchange(new dom.window.Event('change'));
    assert.equal(agent.checked, true); assert.equal(writes.length, 3);
  });
});

test('members cannot see invitations or change roles or resource grants', async () => {
  await fixture('Member', async ({ writes, button }) => {
    assert.equal(button('＋ Invite person').hidden, true);
    button(`View ${member.email}`).click(); await settle();
    assert.equal(document.querySelector('dialog select').disabled, true);
    assert.equal(button('Save role'), undefined); assert.equal(button('Remove from workspace'), undefined);
    assert.equal(document.querySelectorAll('dialog [role=tab]').length, 1);
    assert.equal(writes.length, 0);
  });
});

test('admins can invite members but do not get owner resource controls', async () => {
  await fixture('Admin', async ({ button, writes }) => {
    button('＋ Invite person').click();
    assert.deepEqual([...document.querySelector('dialog select').options].map(o => o.value), ['Member']);
    button('Close').click(); button(`View ${member.email}`).click(); await settle();
    assert.equal(button('Save role'), undefined); assert.ok(button('Remove from workspace'));
    assert.equal(document.querySelectorAll('dialog [role=tab]').length, 1);
    assert.equal(writes.length, 0);
  });
});

test('owner access is implicit and cannot be changed by member controls', async () => {
  await fixture('Owner', async ({ button }) => {
    button(`View ${owner.email}`).click(); await settle();
    assert.equal(document.querySelector('dialog select').disabled, true);
    assert.equal(button('Save role'), undefined); assert.equal(button('Remove from workspace'), undefined);
    assert.equal(document.querySelectorAll('dialog [role=tab]').length, 1);
  });
});

test('tabs support keyboard selection and filtering leaves invitations selected', async () => {
  await fixture('Owner', async ({ dom }) => {
    const tabs = [...document.querySelectorAll('[role=tab]')];
    tabs[0].dispatchEvent(new dom.window.KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
    assert.equal(tabs[1].getAttribute('aria-selected'), 'true'); assert.equal(document.activeElement, tabs[1]);
    const search = document.querySelector('input[type=search]'); search.value = 'missing'; search.dispatchEvent(new dom.window.Event('input'));
    assert.equal(tabs[1].getAttribute('aria-selected'), 'true');
    assert.equal(document.querySelector('[role=tabpanel]:not([hidden])').textContent, 'No invitations match your search.');
  });
});

test('an in-flight invitation prevents duplicate submission and accidental dismissal', async () => {
  let release;
  const pending = new Promise(resolve => { release = resolve; });
  await fixture('Owner', async ({ dom, writes, button }) => {
    button('＋ Invite person').click();
    const form = document.querySelector('form'); form.elements.email.value = 'invited@example.com';
    const first = form.onsubmit(new dom.window.Event('submit'));
    await form.onsubmit(new dom.window.Event('submit'));
    assert.equal(writes.length, 1);
    assert.equal(button('Close').disabled, true);
    assert.equal(form.elements.email.disabled, true);
    const cancel = new dom.window.Event('cancel', { cancelable: true });
    document.querySelector('dialog').dispatchEvent(cancel); assert.equal(cancel.defaultPrevented, true);
    release({ error: 'Try again' }); await first;
    assert.equal(button('Close').disabled, false);
    assert.equal(button('Send invitation').disabled, false);
    assert.equal(form.elements.email.disabled, false);
  }, (path, body) => body && path.endsWith('/invitations') ? { ok: false, data: pending } : undefined);
});
