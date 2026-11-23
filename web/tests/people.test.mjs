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
    // Invitations sit on top of the one list (2026-11-21; they had their own tab).
    assert.match(document.querySelector('.settings-invitation').textContent, /uncertain@example\.com.*Delivery unconfirmed/);
    assert.ok(button('Cancel the invitation for uncertain@example.com'));
    assert.equal(button('Copy a new invite link for uncertain@example.com'), undefined, 'no link while delivery is unconfirmed');
  }, (path, body) => {
    if (body && path.endsWith('/invitations')) {
      current.invitations.push({ id: 'uncertain', email: body.email, role: 'Member', status: 'delivery_unknown', expires: 0 });
      return { ok: false, data: { error: 'Cancel this invitation before trying again.' } };
    }
  });
});

test('A refused email still gives the admin the link to send, marked Link only', async () => {
  let current;
  await fixture('Admin', async ({ people, writes, button, click, change, submit }) => {
    current = people;
    await click(button('＋ Invite person'));
    const form = document.querySelector('form');
    await change(form.elements.email, 'existing@example.com');
    await submit(form);
    const dialog = document.querySelector('dialog');
    assert.match(dialog.textContent, /Invitation ready/);
    assert.match(dialog.textContent, /could not be sent, so send this link to existing@example.com yourself/);
    assert.doesNotMatch(dialog.textContent, /received an email/);
    assert.match(dialog.querySelector('code').textContent, /\/invite\/secret$/);
    assert.equal(writes.length, 1);
    await click(button('Done'));
    const row = document.querySelector('.settings-invitation');
    assert.match(row.textContent, /Link only · expires in 1 day · joins as a member/);
    assert.equal(document.querySelector('.settings-invitation + .settings-person') !== null, true, 'people follow the invitations');
    assert.match(document.querySelector('.desk-title p').textContent, /^2 people · 1 invitation waiting$/);
    assert.ok(button('Copy a new invite link for existing@example.com'));
  }, (path, body) => {
    if (body && path.endsWith('/invitations')) {
      current.invitations.push({ id: 'refused', email: body.email, role: 'Member', status: 'pending', delivery: 'link', expires: Date.now() / 1000 + 600 });
      return { ok: true, data: { id: 'refused', email_sent: false, link: 'http://localhost/invite/secret' } };
    }
  });
});

test('People shows roles and marks you; the menu offers the right role change', async () => {
  await fixture('Admin', async ({ button, click }) => {
    const rows = [...document.querySelectorAll('.settings-person')];
    assert.match(rows[0].textContent, /owner@example.com \(you\)Admin/);
    assert.match(rows[1].textContent, /employee@example.comMember/);
    assert.equal(document.querySelector('.desk-title p').textContent, '2 people');
    assert.equal(document.querySelector('[role=tab]'), null, 'one list, no tabs');
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

test('Removal pauses agents they own alone unless a new owner is named, and sends one request', async () => {
  let release;
  const pending = new Promise(resolve => { release = resolve; });
  const second = { id: 'second', email: 'second@example.com', role: 'Admin' };
  await fixture('Admin', async ({ writes, reads, button, click, change, update }) => {
    await openMenu(click, button, member, 'Remove from workspace…');
    assert.ok(reads.some(path => path.endsWith('/members/employee/responsibilities')));
    const dialog = document.querySelector('dialog');
    assert.equal(dialog.querySelector('h2').textContent, `Remove ${member.email}?`);
    // With no new owner named, agents they own alone pause until an admin gives them one;
    // an agent with other owners stays with them.
    assert.match(dialog.textContent, /Paused until an admin gives them an owner3 agents: Atlas, Nova, Scout/);
    assert.match(dialog.textContent, /Stays with their other owners1 agent: Orbit/);
    const select = label => [...dialog.querySelectorAll('label')].find(node => node.textContent.startsWith(label)).querySelector('select');
    const owner = select('New owner of agents only they own');
    assert.equal(owner.value, '', 'nobody owns them by default');
    assert.deepEqual([...owner.options].map(option => option.value), ['', 'owner', 'second']);
    assert.match(dialog.textContent, /Stays as it is2 private or selected Library items/);
    assert.match(dialog.textContent, /Paused for review2 schedules/);
    assert.match(dialog.textContent, /1 invitation employee@example.com sent/);
    const heir = select('Who takes over their Library items');
    assert.equal(heir.value, 'owner', 'the acting admin takes their Library items by default');
    assert.deepEqual([...heir.options].map(option => option.value), ['owner', 'second']);
    // Naming an owner moves the agents to them; an only-me agent then pauses for review.
    await change(owner, 'second');
    assert.match(dialog.textContent, /Moves to second@example.com3 agents: Atlas, Nova, Scout/);
    assert.match(dialog.textContent, /Stays with their other owners1 agent: Orbit/);
    assert.match(dialog.textContent, /Paused for review1 only-me agent and 2 schedules/);
    const remove = button(`Remove ${member.email}`);
    await click(remove); await click(remove);
    assert.equal(writes.length, 1);
    assert.deepEqual(writes[0].body, { role: null, handover_to: 'owner', agents_owner: 'second' });
    assert.equal(button('Close').disabled, true);
    await update(() => release({ ok: true }));
    assert.equal(document.querySelector('dialog'), null);
    assert.match(document.querySelector('.desk-page > [role=status]').textContent, /was removed/);
  }, (path, body) => {
    if (path.endsWith('/responsibilities')) return { ok: true, data: { ...nothing, agents: [{ id: 'a', name: 'Atlas', alone: true }, { id: 'n', name: 'Nova', alone: true }, { id: 'o', name: 'Orbit', alone: false }, { id: 's', name: 'Scout', alone: true }], private_agents: 1, kept_library_items: 2, schedules: 2, invitations: 1 } };
    if (body && path.endsWith('/members/employee')) return { ok: true, data: pending };
  }, [second]);
});

test('Removing someone whose agents all have other owners asks for no new owner', async () => {
  await fixture('Admin', async ({ writes, button, click }) => {
    await openMenu(click, button, member, 'Remove from workspace…');
    const dialog = document.querySelector('dialog');
    assert.match(dialog.textContent, /Stays with their other owners2 agents: Atlas, Orbit/);
    assert.equal(dialog.textContent.includes('New owner of agents only they own'), false);
    assert.equal(dialog.textContent.includes('Paused until an admin'), false);
    await click(button(`Remove ${member.email}`));
    assert.deepEqual(writes[0].body, { role: null, handover_to: 'owner' });
  }, path => path.endsWith('/responsibilities')
    ? { ok: true, data: { ...nothing, agents: [{ id: 'a', name: 'Atlas', alone: false }, { id: 'o', name: 'Orbit', alone: false }] } } : undefined);
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

test('People show the photo, job title and online dot from the directory, and search covers them', async () => {
  const photo = '0b3c9d2e-6f1a-4c55-9e1d-7a2b3c4d5e6f';
  await fixture('Admin', async ({ change }) => {
    const rows = () => [...document.querySelectorAll('.settings-person')];
    assert.equal(rows()[1].querySelector('img').getAttribute('src'), `/api/identity/workspaces/${workspace}/members/employee/photo/${photo}`);
    assert.match(rows()[1].querySelector('.ui-row-title small').textContent, /^Warehouse lead$/);
    assert.equal(rows()[1].querySelector('.presence-dot') !== null, true);
    assert.equal(rows()[0].querySelector('.presence-dot'), null, 'nobody is shown online without presence');
    await change(document.querySelector('input[type=search]'), 'warehouse');
    assert.deepEqual(rows().map(row => row.querySelector('strong').textContent), ['employee@example.com']);
    await change(document.querySelector('input[type=search]'), 'missing');
    assert.equal(document.querySelector('.ui-empty').textContent, 'Nobody matches your search.');
  }, path => path.endsWith('/chat-directory') ? { ok: true, data: { role: 'Admin', agents: [], users: [{ id: 'employee', name: 'Employee', title: 'Warehouse lead', photo }],
    presence: [{ id: 'employee', online: true, last_seen: Math.floor(Date.now() / 1000), status: null, revision: 1 }] } } : undefined);
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
