import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';
import { outboxStorage } from './outbox-storage-fixture.mjs';
const w = '11111111-1111-4111-8111-111111111111', other = '22222222-2222-4222-8222-222222222222', url = `http://localhost/w/${w}/?view=chat`;
const session = { user: { id: 'meera', email: 'meera@example.test' }, workspaces: [{ id: w, name: 'Studio', role: 'Member' }] };
const minute = 60_000;
const present = (id, online, extra = {}) => ({ id, online, last_seen: Date.now() - minute, status: null, revision: 1, ...extra });
const meeting = { kind: 'meeting', emoji: null, text: null, clear_at: null, set_at: 1 };
const directory = (presence = []) => ({ role: 'Member', presence, users: [
  { id: 'meera', name: 'Meera Iyer', title: 'Design lead', role: 'Member' },
  { id: 'ravi', name: 'Ravi Kumar', title: 'Operations', role: 'Member' },
  { id: 'kavya', name: 'Kavya Nair', title: 'Accounts', role: 'Member' }],
  agents: [{ id: 'atlas', name: 'Atlas', role: 'Orders assistant', audience: 'selected', avatar: 'robot-03' }] });
const dm = (id, users, agents = []) => ({ id, name: id, kind: 'direct', users, agents, description: '' });
const chats = [dm('with-ravi', ['meera', 'ravi']), dm('with-kavya', ['meera', 'kavya']), dm('with-atlas', ['meera'], ['atlas'])];
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
// What a screen reader announces: text without the decorative, aria-hidden parts.
const named = node => { const copy = node.cloneNode(true); copy.querySelectorAll('[aria-hidden=true]').forEach(part => part.remove()); return copy.textContent.replace(/\s+/g, ' ').trim(); };
async function start() { await (await import('../src/workspace-context.ts')).enterWorkspace(); }
function fetcher(people, reads = []) {
  return async path => {
    reads.push(path);
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chat-directory')) return response(directory(people));
    if (path.endsWith('/chats')) return response({ chats, preferences: { available: true, revision: 0, pins: [] } });
    return response({});
  };
}

test('presence from another workspace, another account or an older read is ignored', async () => {
  await reactFixture(async () => {
    await start();
    const store = await import('../src/react/chat/presence.ts');
    store.clearPresence();
    store.applyPresence([present('ravi', true, { revision: 3 })]);
    assert.equal(store.presenceOf('ravi').online, true);
    store.applyPresence([present('ravi', false, { revision: 2 })]);
    assert.equal(store.presenceOf('ravi').online, true, 'an older revision never wins');
    store.applyPresence([present('ravi', false, { revision: 9 })], other);
    assert.equal(store.presenceOf('ravi').online, true, "another workspace's read is dropped");
    store.applyPresence([{ id: 'ravi' }, null, 'junk', present('kavya', true)]);
    assert.equal(store.presenceOf('kavya').online, true, 'malformed entries are skipped');
    store.applyPresence([present('ravi', false, { revision: 4 })]);
    assert.equal(store.presenceOf('ravi').online, false);
    // A status that ended reads as none, as the server would show it.
    store.applyPresence([present('kavya', true, { revision: 2, status: { ...meeting, clear_at: Date.now() - 1 } })]);
    assert.equal(store.presenceOf('kavya').status, null);
    // Another account in this tab starts empty.
    const context = await import('../src/workspace-context.ts');
    const signedIn = context.workspaceSession.user;
    context.workspaceSession.user = { id: 'someone-else', email: 'x@example.test' };
    assert.equal(store.presenceOf('ravi'), undefined);
    store.applyPresence([present('ravi', true, { revision: 1 })]);
    assert.equal(store.presenceOf('ravi').revision, 1);
    context.workspaceSession.user = signedIn;
    assert.equal(store.presenceOf('ravi'), undefined, 'switching back does not reuse the other account’s view');
  }, { url, fetch: fetcher([]) });
});

test('dots and names: online people have a green dot and read "name, online, status"; agents never do', async () => {
  const people = [present('ravi', true, { status: meeting }), present('kavya', false, { last_seen: Date.now() - 5 * minute })];
  await reactFixture(async ({ render, click, button }) => {
    await start();
    (await import('../src/react/chat/presence.ts')).clearPresence();
    const { WorkspaceChat } = await import('../src/react/chat/workspace-chat.tsx');
    await render(WorkspaceChat, { href: `${url}&conversation=with-ravi` });
    const row = name => [...document.querySelectorAll('a.chat-contact')].find(link => link.textContent.includes(name));
    assert.ok(row('Ravi Kumar').querySelector('.face .presence-dot'), 'a dot on the face');
    assert.equal(named(row('Ravi Kumar')), 'Ravi Kumar, online, In a meeting');
    assert.equal(row('Ravi Kumar').querySelector('.presence-status-emoji').getAttribute('title'), 'In a meeting', 'the text on hover');
    assert.equal(row('Kavya Nair').querySelector('.presence-dot'), null, 'away: no dot');
    assert.equal(named(row('Kavya Nair')), 'Kavya Nair');
    assert.equal(row('Atlas').querySelector('.presence-dot'), null, 'agents have no presence');
    // The header shows the other person, not this tab's connection.
    const header = document.querySelector('.chat-pane-header');
    assert.ok(header.querySelector('.chat-header-identity .presence-dot'));
    assert.equal(named(header.querySelector('.chat-header-meta')), 'Online In a meeting');
    assert.match(header.querySelector('.chat-header-meta').textContent, /^Online · 🗓️ In a meeting$/);
    assert.doesNotMatch(header.textContent, /Connect/);
    // The People & agents directory and the profile panel.
    await click(button('People & agents3 · 1'));
    assert.ok(button('View profile of Ravi Kumar, online, In a meeting'));
    assert.ok(button('View profile of Kavya Nair'));
    assert.ok(button('View profile of Atlas'));
    await click(button('View profile of Ravi Kumar, online, In a meeting'));
    const card = document.querySelector('aside.chat-side-panel');
    assert.ok(card.querySelector('.profile-card-face .presence-dot'));
    assert.equal(named(card.querySelector('.profile-card-presence')), 'Online');
    assert.equal(named(card.querySelector('.profile-card-status strong')), 'In a meeting');
    assert.equal(card.querySelector('.profile-card-status small').textContent, 'set by Ravi');
  }, { url, fetch: fetcher(people) });
});

test('the direct message header reads online with the status, last seen, or the job title', async () => {
  await reactFixture(async ({ render, update }) => {
    await start();
    const store = await import('../src/react/chat/presence.ts');
    const { ConversationHeader } = await import('../src/react/chat/conversation-header.tsx');
    const props = chat => ({ chat, directory: directory(), title: 'Ravi Kumar', back() {}, search() {}, levelChanged() {}, details: { open: false, toggle() {}, show() {}, review() {} } });
    const meta = () => named(document.querySelector('.chat-header-meta'));
    store.clearPresence();
    await render(ConversationHeader, props(chats[0]));
    assert.equal(meta(), 'Operations', 'nothing known yet: the job title');
    const seen = new Date(); seen.setHours(9, 58, 0, 0);
    const earlier = seen.getTime() > Date.now() ? seen.getTime() - 86_400_000 : seen.getTime();
    await render(ConversationHeader, props(chats[0]));
    await update(() => store.applyPresence([present('ravi', false, { revision: 2, last_seen: earlier })]));
    await render(ConversationHeader, props(chats[0]));
    const today = new Date(earlier).toDateString() === new Date().toDateString();
    assert.equal(meta(), today ? `Last seen ${new Date(earlier).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })}` : 'Last seen yesterday');
    await update(() => store.applyPresence([present('ravi', true, { revision: 3, status: { kind: 'custom', emoji: '🚚', text: 'Out on deliveries', clear_at: null } })]));
    await render(ConversationHeader, props(chats[0]));
    assert.equal(meta(), 'Online Out on deliveries');
    assert.match(document.querySelector('.chat-header-meta').textContent, /🚚/);
    await update(() => store.applyPresence([present('ravi', true, { revision: 4, status: { kind: 'away', clear_at: null } })]));
    await render(ConversationHeader, props(chats[0]));
    assert.equal(meta(), 'Away', 'Away shows as away even with Cuckoo open');
    assert.equal(document.querySelector('.presence-dot'), null);
    await render(ConversationHeader, props(chats[2]));
    assert.equal(meta(), 'Orders assistant', 'an agent shows its job, never presence');
  }, { url, fetch: fetcher([]) });
});

test('when statuses clear and when people were last seen read naturally', async () => {
  await reactFixture(async () => {
    await start();
    const { clearsLabel, lastSeenLabel } = await import('../src/react/chat/presence-view.tsx');
    const now = new Date(2026, 10, 7, 10, 30).getTime();
    const at = (day, hour, minutes = 0) => new Date(2026, 10, day, hour, minutes).getTime();
    const time = value => new Date(value).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
    assert.equal(clearsLabel({ kind: 'meeting', clear_at: at(7, 11, 30) }, now), `until ${time(at(7, 11, 30))}`);
    assert.equal(clearsLabel({ kind: 'meeting', clear_at: at(8, 0) }, now), 'until tonight');
    assert.equal(clearsLabel({ kind: 'busy', clear_at: at(9, 0) }, now), `until ${new Date(at(9, 0)).toLocaleDateString(undefined, { weekday: 'long' })}`);
    assert.equal(clearsLabel({ kind: 'busy', clear_at: null }, now), '');
    assert.equal(lastSeenLabel(at(7, 9, 58), now), `Last seen ${time(at(7, 9, 58))}`);
    assert.equal(lastSeenLabel(at(6, 18), now), 'Last seen yesterday');
    assert.equal(lastSeenLabel(at(2, 18), now), `Last seen ${new Date(at(2, 18)).toLocaleDateString(undefined, { day: 'numeric', month: 'short' })}`);
  }, { url, fetch: fetcher([]) });
});

test('live presence events update faces without reading the directory again', async () => {
  const previous = { indexedDB: globalThis.indexedDB, EventSource: globalThis.EventSource };
  const streams = [], reads = [];
  globalThis.indexedDB = outboxStorage();
  globalThis.EventSource = class {
    constructor(address) { this.url = address; this.listeners = new Map(); streams.push(this); }
    addEventListener(name, fn) { this.listeners.set(name, fn); }
    close() { this.closed = true; }
    emit(name, data) { this.listeners.get(name)?.({ data: JSON.stringify(data) }); }
  };
  try {
    await reactFixture(async ({ render, update }) => {
      await start();
      (await import('../src/react/chat/presence.ts')).clearPresence();
      const live = await import('../src/react/chat/live.ts');
      const { ParticipantFace } = await import('../src/react/chat/conversation-face.tsx');
      const faces = () => createElement('div', null,
        createElement(ParticipantFace, { kind: 'user', who: { id: 'ravi', name: 'Ravi Kumar' }, presence: true }),
        createElement(ParticipantFace, { kind: 'agent', who: { id: 'atlas', name: 'Atlas' }, presence: true }));
      const stop = live.startLive();
      try {
        await render(faces);
        await update(() => {});
        assert.equal(streams.length, 1);
        const before = reads.filter(path => path.endsWith('/chat-directory')).length;
        await update(() => streams[0].emit('presence', { people: [present('ravi', true, { revision: 5 })] }));
        assert.equal(document.querySelectorAll('.presence-dot').length, 1, 'the person, not the agent');
        await update(() => streams[0].emit('presence', { people: [present('ravi', false, { revision: 6 })] }));
        assert.equal(document.querySelectorAll('.presence-dot').length, 0);
        await update(() => streams[0].emit('presence', { people: [present('ravi', true, { revision: 5 })] }));
        assert.equal(document.querySelectorAll('.presence-dot').length, 0, 'a late, older change is ignored');
        await update(() => streams[0].listeners.get('presence')?.({ data: 'not json' }));
        assert.equal(reads.filter(path => path.endsWith('/chat-directory')).length, before, 'no refetch');
      } finally { stop?.(); }
    }, { url: `http://localhost/w/${w}/`, fetch: async path => {
      reads.push(path);
      if (path.endsWith('/session')) return response(session);
      if (path.endsWith('/message-capabilities')) return response({ live: true });
      return response({ ok: true });
    } });
  } finally { Object.assign(globalThis, previous); }
});

test('the connection banner shows only while offline or reconnecting', async () => {
  await reactFixture(async ({ render }) => {
    await start();
    const { ConnectionBanner } = await import('../src/react/chat/connection-banner.tsx');
    const shown = async status => { await render(ConnectionBanner, { status }); return document.querySelector('.chat-connection-banner'); };
    for (const quiet of ['Connected', 'Connecting…', 'Standard refresh · live delivery requires PostgreSQL', 'Local message storage unavailable. Keep your draft.']) {
      assert.equal(await shown(quiet), null, quiet);
    }
    const offline = await shown('Offline · messages are kept on this device until confirmed.');
    assert.equal(offline.getAttribute('role'), 'status');
    assert.match(named(offline), /^You’re offline\. Messages you send will go out when you’re back\.\s*Retry$/);
    for (const trying of ['Reconnecting… Messages remain on this device until confirmed.', 'Reconnecting…', 'Cannot reach server. Pending messages are kept on this device.']) {
      assert.equal(named(await shown(trying)), 'Reconnecting…', trying);
    }
  }, { url, fetch: fetcher([]) });
});

test('Busy quietens alerts to direct messages and what is for you, until it clears', async () => {
  await reactFixture(async () => {
    await start();
    const store = await import('../src/react/chat/presence.ts');
    const { shouldAlert, quietened } = await import('../src/react/chat/notifications.ts');
    const now = 1000, base = { id: 'm', chat: 'orders', author: 'ravi', name: 'Ravi', kind: 'user', content: 'Hi', state: 'sent', at: now };
    const channel = { id: 'orders', name: 'orders', kind: 'channel', users: [], agents: [] }, direct = chats[0];
    store.clearPresence();
    assert.equal(quietened(), false);
    assert.equal(shouldAlert(base, channel, 'all', false, now), true);
    store.applyPresence([present('meera', true, { revision: 2, status: { kind: 'busy', clear_at: null } })]);
    assert.equal(quietened(), true);
    assert.equal(shouldAlert(base, channel, 'all', false, now), false, 'channel chatter is quiet');
    assert.equal(shouldAlert({ ...base, for_me: true }, channel, 'all', false, now), true, '@mentions still alert');
    assert.equal(shouldAlert({ ...base, chat: 'with-ravi' }, direct, 'all', false, now), true, 'direct messages still alert');
    assert.equal(shouldAlert({ ...base, for_me: true }, channel, 'none', false, now), false, 'muted stays muted');
    store.applyPresence([present('meera', true, { revision: 3, status: { kind: 'busy', clear_at: Date.now() - 1 } })]);
    assert.equal(shouldAlert(base, channel, 'all', false, now), true, 'an ended Busy no longer quietens');
    store.applyPresence([present('meera', true, { revision: 4, status: { kind: 'meeting', clear_at: null } })]);
    assert.equal(shouldAlert(base, channel, 'all', false, now), true, 'only Busy quietens');
  }, { url, fetch: fetcher([]) });
});
