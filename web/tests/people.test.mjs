import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

const workspace = '11111111-1111-4111-8111-111111111111';
const member = { id: 'employee', email: 'employee@example.com', role: 'Member' };
const owner = { id: 'owner', email: 'owner@example.com', role: 'Admin' };
const nothing = { agents: [], private_agents: 0, library_items: 0, kept_library_items: 0, schedules: 0, event_triggers: 0,  invitations: 0 };

// The signed-in person is `owner`. `extra` adds people to the workspace.
async function fixture(viewerRole, run, respond, extra = []) {
  const writes = [], reads = [];
  const people = { role: viewerRole, members: [{ ...owner }, { ...member }, ...extra], invitations: [] };
  await reactFixture(async context => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { PeoplePage } = await import('../src/react/people.tsx');
    await context.render(PeoplePage);
    await run({ ...context, writes, reads, people });
  }, { url: `http://localhost/w/${workspace}/?view=people`, fetch: async (path, options = {}) => {
    const body = options.body ? JSON.parse(options.body) : undefined;
    if (body !== undefined) {
      assert.equal(options.headers['X-Ayati-Request'], 'identity');
      assert.ok(path.startsWith(`/api/identity/workspaces/${workspace}/`)); writes.push({ path, body });
    } else reads.push(path);
    const custom = respond?.(path, body);
    if (custom) return { ok: custom.ok, json: async () => custom.data };
    if (path.endsWith('/session')) return { ok: true, json: async () => ({ user: { id: 'owner', email: owner.email, display_name: null }, workspaces: [{ id: workspace, name: 'Store', role: viewerRole }] }) };
    if (path.endsWith('/people')) return { ok: true, json: async () => people };
    if (path.endsWith('/responsibilities')) return { ok: true, json: async () => nothing };
    if (path.endsWith('/chat-directory')) return { ok: true, json: async () => ({ role: viewerRole, agents: [{ id: 'agent', name: '<script>Agent</script>' }], grants: [] }) };
    return { ok: true, json: async () => ({}) };
  } });
}
const tab = name => [...document.querySelectorAll('[role=tab]')].find(node => node.textContent.startsWith(name));
const menuItem = name => [...document.querySelectorAll('[role=menuitem]')].find(node => node.textContent === name);
async function openMenu(click, button, person, item) {
  await click(button(`Actions for ${person.email}`));
  await click(menuItem(item));
}

test('React invitations retain failed form values and require an explicit retry', async () => {
  let failing = true;
  await fixture('Admin', async ({ writes, button, click, change, submit }) => {
    assert.equal(document.querySelector('form'), null);
    await click(button('＋ Invite person'));
    const form = document.querySelector('form');
    assert.equal(document.activeElement, form.elements.email);
    await change(form.elements.email, 'invited@example.com');
    assert.equal(writes.length, 0); await submit(form);
    assert.match(document.querySelector('dialog [role=status]').textContent, /Invitation failed/);
    assert.equal(form.elements.email.value, 'invited@example.com'); assert.equal(button('Send invitation').disabled, false);
    assert.equal(writes.length, 1); failing = false; await submit(form);
    assert.equal(document.querySelector('dialog'), null);
    assert.deepEqual(writes[1].body, { email: 'invited@example.com', role: 'Member' });
  }, (path, body) => body && path.endsWith('/invitations') && failing ? { ok: false, data: { error: 'Invitation failed' } } : undefined);
});

test('Unconfirmed delivery refreshes invitations without losing the failed draft', async () => {
  let current;
  await fixture('Admin', async ({ people, writes, button, click, change, submit }) => {
    current = people;
    await click(button('＋ Invite person'));
    const form = document.querySelector('form');
    await change(form.elements.email, 'uncertain@example.com');
    await submit(form);
    assert.equal(form.elements.email.value, 'uncertain@example.com');
    assert.match(document.querySelector('dialog [role=status]').textContent, /Cancel this invitation/);
    assert.equal(writes.length, 1);
    await click(button('Close'));
    await click(tab('Invitations'));
    assert.match(document.querySelector('[role=tabpanel]:not([hidden])').textContent, /Delivery unconfirmed/);
    assert.ok(button('Cancel invitation'));
  }, (path, body) => {
    if (body && path.endsWith('/invitations')) {
      current.invitations.push({ id: 'uncertain', email: body.email, role: 'Member', status: 'delivery_unknown', expires: 0 });
      return { ok: false, data: { error: 'Cancel this invitation before trying again.' } };
    }
  });
});

test('People shows roles and marks you; the menu offers the right role change', async () => {
  await fixture('Admin', async ({ button, click }) => {
    const rows = [...document.querySelectorAll('.settings-person')];
    assert.match(rows[0].textContent, /owner@example.com \(you\)Admin/);
    assert.match(rows[1].textContent, /employee@example.comMember/);
    assert.match(tab('People').textContent, /People · 2/);
    await click(button(`Actions for ${member.email}`));
    assert.ok(menuItem('Make admin')); assert.ok(menuItem('Remove from workspace…'));
    await click(button(`Actions for ${owner.email}`));
    assert.equal(document.querySelectorAll('[role=menu]').length, 1, 'opening another menu closes the first');
    assert.ok(!menuItem('Remove from workspace…'), 'you cannot remove yourself here');
  });
});

test('Agent access opens from the menu, grants nothing on open, and keeps saved access after a rejection', async () => {
  await fixture('Admin', async ({ writes, button, click }) => {
    await openMenu(click, button, member, 'Agent access…');
    assert.equal(document.querySelector('dialog h2').textContent, `Agent access · ${member.email}`);
    assert.equal(writes.length, 0); assert.equal(document.querySelector('script'), null);
    const agent = document.querySelector('dialog input[type=checkbox]'); await click(agent);
    assert.equal(agent.checked, false); assert.equal(agent.matches(':disabled'), false);
    assert.deepEqual(writes[0].body, { user: 'employee', agent: 'agent', allowed: true });
    assert.equal(writes.length, 1);
  }, (_path, body) => body ? { ok: false, data: { error: 'Access was revoked' } } : undefined);
});

test('Making someone an admin asks first and changes only that person', async () => {
  await fixture('Admin', async ({ writes, button, click }) => {
    await openMenu(click, button, member, 'Make admin');
    assert.equal(document.querySelector('dialog h2').textContent, `Make ${member.email} an admin?`);
    assert.equal(writes.length, 0);
    await click(document.querySelector('dialog footer .primary'));
    assert.deepEqual(writes, [{ path: `/api/identity/workspaces/${workspace}/members/employee`, body: { role: 'Admin' } }]);
    assert.equal(document.querySelector('dialog'), null);
    assert.match(document.querySelector('.desk-page > [role=status]').textContent, /is now an admin/);
  });
});

test('Members cannot open workspace management controls', async () => {
  await fixture('Member', async ({ writes }) => {
    assert.match(document.body.textContent, /Only admins/);
    assert.equal(document.querySelector('input'), null);
    assert.equal(writes.length, 0);
  });
});

test('The only admin cannot be made a member or removed', async () => {
  await fixture('Admin', async ({ button, click }) => {
    await click(button(`Actions for ${owner.email}`));
    assert.equal(menuItem('Make member').disabled, true);
    assert.match(document.querySelector('[role=menu]').textContent, /The only admin can’t be made a member or removed/);
  });
});

test('Removal shows what moves, hands it to the chosen person and sends one request', async () => {
  let release;
  const pending = new Promise(resolve => { release = resolve; });
  const second = { id: 'second', email: 'second@example.com', role: 'Admin' };
  await fixture('Admin', async ({ writes, reads, button, click, change, update }) => {
    await openMenu(click, button, member, 'Remove from workspace…');
    assert.ok(reads.some(path => path.endsWith('/members/employee/responsibilities')));
    const dialog = document.querySelector('dialog');
    assert.equal(dialog.querySelector('h2').textContent, `Remove ${member.email}?`);
    assert.match(dialog.textContent, /Moves to owner@example.com3 agents: Atlas, Nova, Scout/);
    assert.match(dialog.textContent, /Stays as it is2 private or selected Library items/);
    assert.match(dialog.textContent, /Paused for review1 only-me agent and 2 schedules/);
    assert.match(dialog.textContent, /1 invitation employee@example.com sent/);
    const heir = dialog.querySelector('select');
    assert.equal(heir.value, 'owner', 'the acting admin takes over by default');
    assert.deepEqual([...heir.options].map(option => option.value), ['owner', 'second']);
    await change(heir, 'second');
    assert.match(dialog.textContent, /Moves to second@example.com/);
    const remove = button(`Remove ${member.email}`);
    await click(remove); await click(remove);
    assert.equal(writes.length, 1);
    assert.deepEqual(writes[0].body, { role: null, handover_to: 'second' });
    assert.equal(button('Close').disabled, true);
    await update(() => release({ ok: true }));
    assert.equal(document.querySelector('dialog'), null);
    assert.match(document.querySelector('.desk-page > [role=status]').textContent, /was removed/);
  }, (path, body) => {
    if (path.endsWith('/responsibilities')) return { ok: true, data: { ...nothing, agents: [{ id: 'a', name: 'Atlas' }, { id: 'n', name: 'Nova' }, { id: 's', name: 'Scout' }], private_agents: 1, kept_library_items: 2, schedules: 2, invitations: 1 } };
    if (body && path.endsWith('/members/employee')) return { ok: true, data: pending };
  }, [second]);
});

test('A failed removal keeps the dialog open with the reason and allows a retry', async () => {
  let failing = true;
  await fixture('Admin', async ({ writes, button, click }) => {
    await openMenu(click, button, member, 'Remove from workspace…');
    assert.match(document.querySelector('dialog').textContent, /doesn’t own or look after anything/);
    await click(button(`Remove ${member.email}`));
    assert.match(document.querySelector('dialog [role=status]').textContent, /Choose someone else to take over/);
    assert.equal(button(`Remove ${member.email}`).disabled, false);
    failing = false; await click(button(`Remove ${member.email}`));
    assert.equal(writes.length, 2); assert.equal(document.querySelector('dialog'), null);
  }, (path, body) => body && path.endsWith('/members/employee') && failing ? { ok: false, data: { error: 'Choose someone else to take over' } } : undefined);
});

test('React tabs support keyboard selection and preserve the active tab during search', async () => {
  await fixture('Admin', async ({ dom, update, change }) => {
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
  await fixture('Admin', async ({ dom, writes, button, click, change, submit, update }) => {
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

test('A late agent-access read after closing is ignored and focus returns to the person’s menu', async () => {
  let release;
  const pending = new Promise(resolve => { release = resolve; });
  await fixture('Admin', async ({ button, click, update, writes }) => {
    await openMenu(click, button, member, 'Agent access…'); await click(button('Close'));
    assert.equal(document.activeElement?.getAttribute('aria-label'), `Actions for ${member.email}`);
    await update(() => release({ role: 'Admin', agents: [], grants: [] }));
    assert.equal(document.querySelector('dialog'), null); assert.equal(writes.length, 0);
  }, path => path.endsWith('/chat-directory') ? { ok: true, data: pending } : undefined);
});
