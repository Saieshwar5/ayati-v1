import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

// Channel Details and profiles in the side panel (D245, screens 5, 6 and 2).
const w = '11111111-1111-4111-8111-111111111111', base = `http://localhost/w/${w}/?view=chat`;
const people = [
  { id: 'priya', name: 'Priya Shah', title: 'Founder', role: 'Admin', email: 'priya@example.test' },
  { id: 'meera', name: 'Meera Iyer', title: 'Design lead', role: 'Member', about: 'Leads residential interiors.', email: 'meera@example.test' },
  { id: 'ravi', name: 'Ravi Kumar', title: 'Operations', role: 'Member' },
  { id: 'arjun', name: 'Arjun Mehta', title: 'Sales', role: 'Member' },
];
const agents = [
  { id: 'atlas', name: 'Atlas', role: 'Orders assistant', audience: 'selected', avatar: 'robot-03', manager: 'Priya Shah' },
  { id: 'nova', name: 'Nova', role: 'Research', audience: 'everyone', avatar: 'robot-05', manager: 'Meera Iyer' },
  { id: 'solo', name: 'Solo', role: 'Private helper', audience: 'only_me', avatar: 'robot-07', manager: 'Meera Iyer' },
];
const orders = { id: 'orders', name: 'orders', kind: 'channel', visibility: 'public', description: 'Orders from confirmation to delivery.', users: ['priya', 'meera', 'ravi'], agents: ['atlas'], can_archive: true, announcements: false };
const general = { id: 'general', name: 'general', kind: 'channel', visibility: 'private', description: '', users: ['priya', 'meera'], agents: [], can_archive: true };
const direct = { id: 'dm-meera', name: 'dm', kind: 'direct', users: ['priya', 'meera'], agents: [], description: '' };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const held = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };

/** A workspace API: GETs from fixtures, POSTs (and chosen GETs) handed to the test. */
function api({ user = 'priya', role = 'Admin', chats = () => [orders, general, direct], post = () => response({ ok: true }), get = () => undefined } = {}) {
  const session = { user: { id: user, email: `${user}@example.test` }, workspaces: [{ id: w, name: 'Lotus Interiors', role }] };
  return async (path, options = {}) => {
    if (options.method === 'POST') return post(path.replace(`/api/identity/workspaces/${w}`, ''), JSON.parse(options.body));
    const custom = get(path.replace(`/api/identity/workspaces/${w}`, ''));
    if (custom) return custom;
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chat-directory')) return response({ role, users: people, agents });
    if (path.endsWith('/chats')) return response({ chats: chats(), preferences: { available: true, revision: 0, pins: [] } });
    if (path.endsWith('/unread')) return response({ unread: [] });
    if (path.endsWith('/chat-notifications')) return response({ levels: {} });
    if (path.includes('/messages')) return response({ messages: [], agents: [] });
    return response({});
  };
}
async function open(render, conversation) {
  await (await import('../src/workspace-context.ts')).enterWorkspace();
  const { WorkspaceChat } = await import('../src/react/chat/workspace-chat.tsx');
  await render(WorkspaceChat, { href: `${base}&conversation=${conversation}` });
  return WorkspaceChat;
}
const panel = () => document.querySelector('aside.chat-side-panel');
const inPanel = label => [...(panel()?.querySelectorAll('button') ?? [])].find(node => node.textContent === label || node.getAttribute('aria-label') === label);
const memberNames = () => [...panel().querySelectorAll('.chat-details-members .chat-name-text')].map(node => node.textContent);
const escape = (dom, update) => update(() => document.activeElement.dispatchEvent(new dom.window.KeyboardEvent('keydown', { key: 'Escape', bubbles: true })));

test('Details opens as a panel beside the conversation, takes focus, and Escape or Close returns it to the trigger', async () => {
  await reactFixture(async ({ render, button, click, update, dom }) => {
    await open(render, 'orders');
    const trigger = button('Details');
    assert.equal(trigger.getAttribute('aria-expanded'), 'false');
    await click(trigger);
    assert.equal(document.querySelectorAll('dialog').length, 0, 'not a modal dialog');
    assert.equal(Boolean(panel()), true);
    assert.equal(panel().parentElement.classList.contains('workspace-chat'), true, 'inside the workspace frame');
    assert.equal(panel().previousElementSibling.classList.contains('chat-content'), true, 'beside the conversation');
    assert.equal(panel().getAttribute('aria-label'), 'orders details');
    assert.equal(trigger.getAttribute('aria-expanded'), 'true');
    assert.equal(panel().contains(document.activeElement), true, 'focus moves into the panel');
    assert.equal(document.activeElement.textContent, '#orders', 'on its title');
    await escape(dom, update);
    assert.equal(Boolean(panel()), false);
    assert.equal(document.activeElement === trigger, true, 'focus returns to Details');
    await click(trigger);
    await click(inPanel('Close orders details'));
    assert.equal(Boolean(panel()), false);
    assert.equal(document.activeElement === trigger, true);
    // The faces in the header open the same panel.
    await click(button('People and agents in orders'));
    assert.equal(Boolean(panel()), true);
  }, { url: `${base}&conversation=orders`, fetch: api() });
});

test('members are listed once with faces and AI tags; managers see add, remove and one settings Save', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click, change }) => {
    await open(render, 'orders');
    await click(button('Details'));
    assert.deepEqual(memberNames(), ['Priya Shah (you)', 'Atlas', 'Meera Iyer', 'Ravi Kumar']);
    assert.equal(panel().textContent.split('Meera Iyer').length - 1, 1, 'each member appears once');
    assert.equal(panel().querySelectorAll('input[type=checkbox]').length, 0, 'no checkbox list of the workspace');
    const atlas = [...panel().querySelectorAll('.chat-details-member')].find(row => row.textContent.includes('Atlas'));
    assert.equal(atlas.querySelector('.ai-tag').textContent, 'AI');
    assert.equal(atlas.querySelector('img').getAttribute('src'), '/agent-avatars/robot-03.svg');
    assert.match(atlas.textContent, /Orders assistant · managed by Priya Shah/);
    const priya = [...panel().querySelectorAll('.chat-details-member')].find(row => row.textContent.includes('Priya'));
    assert.match(priya.textContent, /Manager/);
    assert.match(panel().textContent, /Orders from confirmation to delivery\./);
    assert.match(panel().textContent, /Public · everyone in Lotus Interiors can find it/);
    assert.match(panel().textContent, /3 people · 1 agent/);
    assert.equal(Boolean(inPanel('Add people')) && Boolean(inPanel('Add agent')) && Boolean(inPanel('Archive')), true);
    assert.equal([...panel().querySelectorAll('button')].filter(node => /^Sav/.test(node.textContent)).length, 1, 'one Save');
    await change(panel().querySelector('input[type=search]'), 'oper');
    assert.deepEqual(memberNames(), ['Ravi Kumar']);
    await change(panel().querySelector('input[type=search]'), 'zzz');
    assert.match(panel().textContent, /Nobody in #orders matches “zzz”/);
    assert.equal(writes.length, 0, 'reading never writes');
  }, { url: `${base}&conversation=orders`, fetch: api({ post: (path, body) => { writes.push({ path, body }); return response({ ok: true }); } }) });
});

test('non-managers see the list without add, remove, archive or settings, and View profile opens in the same panel', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click }) => {
    await open(render, 'orders');
    await click(button('Details'));
    assert.deepEqual(memberNames(), ['Ravi Kumar (you)', 'Atlas', 'Meera Iyer', 'Priya Shah']);
    for (const label of ['Add people', 'Add agent', 'Archive']) assert.equal(Boolean(inPanel(label)), false, label);
    assert.doesNotMatch(panel().textContent, /Channel settings/);
    await click(inPanel('Actions for Meera Iyer'));
    assert.deepEqual([...panel().querySelectorAll('[role=menuitem]')].map(node => node.textContent), ['Message', 'View profile']);
    await click(inPanel('View profile'));
    assert.equal(panel().getAttribute('aria-label'), 'Profile');
    assert.match(panel().textContent, /Leads residential interiors\./);
    assert.equal(panel().contains(document.activeElement), true);
    await click(inPanel('Back to orders details'));
    assert.equal(panel().getAttribute('aria-label'), 'orders details');
    assert.equal(writes.length, 0);
  }, { url: `${base}&conversation=orders`, fetch: api({ user: 'ravi', role: 'Member', chats: () => [{ ...orders, can_archive: false }],
    post: (path, body) => { writes.push({ path, body }); return response({ ok: true }); } }) });
});

test('a manager adds a person and an agent with one request each, a double click sends once, and Remove sends the rest', async () => {
  const writes = []; let pending;
  await reactFixture(async ({ render, button, click, update }) => {
    await open(render, 'orders');
    await click(button('Details'));
    await click(inPanel('Add people'));
    assert.equal(document.activeElement.getAttribute('aria-label'), 'Search people to add', 'focus moves to the search');
    assert.deepEqual([...panel().querySelectorAll('.chat-details-adding .chat-name-text')].map(node => node.textContent), ['Arjun Mehta']);
    const add = inPanel('Add Arjun Mehta');
    await update(() => { add.click(); add.click(); });
    assert.deepEqual(writes, [{ path: '/chats/orders/members', body: { users: ['priya', 'meera', 'ravi', 'arjun'] } }]);
    await update(() => pending.resolve(response({ ok: true })));
    assert.match(panel().textContent, /Everyone in the workspace is already here\./);
    assert.match(panel().textContent, /Added Arjun Mehta to #orders\./);
    await click(inPanel('Done'));
    assert.equal(document.activeElement === inPanel('Add people'), true, 'focus returns to Add people');
    assert.deepEqual(memberNames(), ['Priya Shah (you)', 'Atlas', 'Arjun Mehta', 'Meera Iyer', 'Ravi Kumar']);
    await click(inPanel('Add agent'));
    assert.deepEqual([...panel().querySelectorAll('.chat-details-adding .chat-name-text')].map(node => node.textContent), ['Nova'], 'only-me agents cannot join');
    await click(inPanel('Add Nova'));
    await update(() => pending.resolve(response({ ok: true })));
    assert.deepEqual(writes[1], { path: '/chats/orders/agents', body: { agents: ['atlas', 'nova'] } });
    await click(inPanel('Done'));
    await click(inPanel('Actions for Priya Shah'));
    assert.equal(Boolean(inPanel('Remove from #orders')), false, 'managers stay in the channels they manage');
    await click(inPanel('Actions for Ravi Kumar'));
    await click(inPanel('Remove from #orders'));
    await update(() => pending.resolve(response({ ok: true })));
    assert.deepEqual(writes[2], { path: '/chats/orders/members', body: { users: ['priya', 'meera', 'arjun'] } });
    assert.equal(memberNames().includes('Ravi Kumar'), false);
    assert.equal(writes.length, 3);
  }, { url: `${base}&conversation=orders`, fetch: api({ post: (path, body) => { writes.push({ path, body }); pending = held(); return pending.promise; } }) });
});

test('channel settings save together once; a failed save keeps the values and shows the error', async () => {
  const writes = []; let pending, current = orders;
  await reactFixture(async ({ render, button, click, change, update }) => {
    await open(render, 'orders');
    await click(button('Details'));
    const save = () => inPanel('Save changes') ?? inPanel('Saving…');
    assert.equal(save().disabled, true, 'nothing to save yet');
    const name = panel().querySelector('.chat-details-form input:not([type=radio])');
    await change(name, 'orders-team');
    await click(panel().querySelector('input[type=radio][value=private]'));
    await change(panel().querySelector('.chat-details-form select'), 'admins');
    assert.match(panel().textContent, /3 unsaved changes/);
    await update(() => { save().click(); save().click(); });
    assert.deepEqual(writes, [{ path: '/chats/orders/settings', body: { name: 'orders-team', visibility: 'private', announcements: true } }]);
    await update(() => pending.resolve(response({ error: 'Settings could not be saved' }, 503)));
    assert.match(panel().querySelector('[role=alert]').textContent, /Settings could not be saved/);
    assert.equal(name.value, 'orders-team');
    assert.equal(panel().querySelector('input[type=radio][value=private]').checked, true);
    assert.equal(panel().querySelector('.chat-details-form select').value, 'admins');
    assert.equal(save().disabled, false);
    await click(save());
    assert.deepEqual(writes[1], writes[0]);
    current = { ...orders, name: 'orders-team', visibility: 'private', announcements: true };
    await update(() => pending.resolve(response({ ok: true })));
    assert.match(panel().textContent, /Saved\./);
    assert.doesNotMatch(panel().textContent, /unsaved/);
    assert.match(panel().textContent, /Only admins can post/);
    assert.equal(writes.length, 2);
  }, { url: `${base}&conversation=orders`, fetch: api({ chats: () => [current, general, direct], post: (path, body) => { writes.push({ path, body }); pending = held(); return pending.promise; } }) });
});

test('archive review opens in place inside Details and lists the unfinished work', async () => {
  const writes = []; let archived = false;
  await reactFixture(async ({ render, button, click, update, dom }) => {
    await open(render, 'orders');
    await click(button('Details'));
    const archive = inPanel('Archive');
    await click(archive);
    assert.equal(document.querySelectorAll('dialog').length, 0, 'no stacked modal');
    const review = panel().querySelector('.chat-archive-review');
    assert.equal(Boolean(review), true);
    assert.equal(review.contains(document.activeElement), true, 'focus moves into the review');
    assert.match(review.textContent, /Archive #orders\?/);
    assert.match(review.textContent, /Still running · 2/);
    assert.match(review.textContent, /2 visible requests have not finished/);
    await click(inPanel('Cancel'));
    assert.equal(Boolean(panel().querySelector('.chat-archive-review')), false);
    assert.equal(document.activeElement === archive, true, 'focus returns to Archive');
    await click(archive);
    await click(inPanel('Archive channel'));
    assert.equal(writes.length, 1);
    assert.equal(writes[0].path, '/chats/orders/lifecycle');
    assert.equal(writes[0].body.archived, true);
    assert.equal(writes[0].body.revision, 4);
    assert.equal(Boolean(panel().querySelector('.chat-archive-review')), false, 'the review closes once archived');
    assert.equal(Boolean(inPanel('Restore')), true);
    assert.match(document.querySelector('.chat-archive-banner').textContent, /archived/);
    // The banner's Restore opens the same review in the panel.
    await click(inPanel(`Close orders details`));
    const restore = button('Restore channel');
    await click(restore);
    assert.match(panel().querySelector('.chat-archive-review').textContent, /Restore #orders\?/);
    assert.equal(panel().querySelector('.chat-archive-review').contains(document.activeElement), true, 'focus lands in the review');
    await escape(dom, update);
    assert.equal(document.activeElement === restore, true, 'and returns to the banner');
  }, { url: `${base}&conversation=orders`, fetch: api({
    chats: () => [{ ...orders, archived }, general, direct],
    get: path => path === '/chats/orders/lifecycle' ? response({ archived, revision: archived ? 5 : 4, active_requests: 2, manageable: true }) : undefined,
    post: (path, body) => { writes.push({ path, body }); archived = body.archived; return response({ archived, revision: 5 }); },
  }) });
});

test('a direct message shows the person’s profile with notifications and Pin, and no member list, settings or archive', async () => {
  await reactFixture(async ({ render, button, click }) => {
    await open(render, 'dm-meera');
    await click(button('Profile'));
    assert.equal(panel().getAttribute('aria-label'), 'Meera Iyer profile');
    const text = panel().textContent;
    for (const shown of [/Leads residential interiors\./, /meera@example\.test/, /Design lead/, /Direct messages have no member list, settings or archive\./]) assert.match(text, shown);
    for (const hidden of [/Channel settings/, /People and agents/, /Archive/, /Add people/]) assert.doesNotMatch(text, hidden);
    assert.equal(Boolean(inPanel('Notifications: All messages')), true, 'direct messages default to all');
    assert.equal(Boolean(inPanel('Pin')), true);
    assert.equal(Boolean(inPanel('Message')), false, 'already messaging them');
  }, { url: `${base}&conversation=dm-meera`, fetch: api() });
});

test('a late reply for the previous conversation never replaces the open one', async () => {
  const late = held();
  await reactFixture(async ({ render, button, click, update }) => {
    const WorkspaceChat = await open(render, 'orders');
    await click(button('Details'));
    await click(inPanel('Archive'));
    assert.match(panel().textContent, /Checking unfinished work/);
    await render(WorkspaceChat, { href: `${base}&conversation=general` });
    assert.equal(panel().getAttribute('aria-label'), 'general details', 'Details follows the open conversation');
    assert.equal(Boolean(panel().querySelector('.chat-archive-review')), false, 'the review stays with its own channel');
    await click(inPanel('Archive'));
    assert.match(panel().textContent, /Nothing is still running/);
    await update(() => late.resolve(response({ archived: false, revision: 9, active_requests: 7, manageable: true })));
    assert.doesNotMatch(panel().textContent, /Still running · 7/);
    assert.match(panel().textContent, /Archive #general\?/);
    assert.match(panel().textContent, /Nothing is still running/);
  }, { url: `${base}&conversation=orders`, fetch: api({
    get: path => path === '/chats/orders/lifecycle' ? late.promise
      : path === '/chats/general/lifecycle' ? response({ archived: false, revision: 1, active_requests: 0, manageable: true }) : undefined,
  }) });
});
