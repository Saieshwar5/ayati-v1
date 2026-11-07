import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
const w = '11111111-1111-4111-8111-111111111111';
const ok = data => ({ ok: true, status: 200, json: async () => data });
const named = node => { const copy = node.cloneNode(true); copy.querySelectorAll('[aria-hidden=true]').forEach(part => part.remove()); return copy.textContent.replace(/\s+/g, ' ').trim(); };
const user = { id: 'priya', email: 'priya@example.com', display_name: 'Priya Shah' };
const dialog = () => document.querySelector('dialog');
const radio = label => [...dialog().querySelectorAll('label')].find(node => named(node).startsWith(label)).querySelector('input');

/** The rail and its account menu; `answer` replies to POST /status. */
async function shell(run, { answer, presence = [] } = {}) {
  const writes = [];
  await reactFixture(async context => {
    context.dom.window.HTMLElement.prototype.hidePopover = function () {};
    await (await import('../src/workspace-context.ts')).enterWorkspace();
    (await import('../src/react/chat/presence.ts')).clearPresence();
    const { WorkspaceShell } = await import('../src/react/workspace-shell.tsx');
    await context.render(WorkspaceShell, { page: 'home', children: 'Home' });
    await run({ ...context, writes });
  }, { url: `http://localhost/w/${w}/`, fetch: async (path, options = {}) => {
    if (options.method === 'POST') {
      const body = JSON.parse(options.body);
      writes.push({ path, body });
      return answer ? answer(body) : ok({ id: 'priya', online: true, last_seen: Date.now(), status: body.status && { ...body.status, set_at: Date.now() }, revision: writes.length + 1 });
    }
    if (path.endsWith('/chat-directory')) return ok({ role: 'Admin', agents: [], users: [{ id: 'priya', name: 'Priya Shah' }], presence });
    return ok({ user, workspaces: [{ id: w, name: 'Lotus Interiors', role: 'Admin' }] });
  } });
}

test('clear after works out the end in your own time: an hour, midnight, the end of Sunday or never', async () => {
  await reactFixture(async () => {
    const { clearAt, chosenStatus } = await import('../src/react/chat/status-dialog.tsx');
    const wednesday = new Date(2026, 10, 4, 10, 30), sunday = new Date(2026, 10, 8, 22, 0);
    assert.equal(clearAt('hour', wednesday), wednesday.getTime() + 3_600_000);
    assert.equal(clearAt('today', wednesday), new Date(2026, 10, 5).getTime());
    assert.equal(clearAt('week', wednesday), new Date(2026, 10, 9).getTime(), 'Monday at midnight');
    assert.equal(clearAt('week', sunday), new Date(2026, 10, 9).getTime());
    assert.equal(clearAt('never', wednesday), null);
    assert.deepEqual(chosenStatus('meeting', '', '', 'today', wednesday), { kind: 'meeting', emoji: null, text: null, clear_at: new Date(2026, 10, 5).getTime() });
    assert.deepEqual(chosenStatus('meeting', '  Out on deliveries ', '🚚', 'never', wednesday), { kind: 'custom', emoji: '🚚', text: 'Out on deliveries', clear_at: null }, 'your own words win');
    assert.equal(chosenStatus('available', '   ', '🚚', 'today', wednesday), null, 'Available clears the status');
  }, { url: `http://localhost/w/${w}/` });
});

test('the status line opens Set a status; Save sends the choice once and the line shows it', async () => {
  let release;
  await shell(async ({ click, writes, update }) => {
    const line = () => document.querySelector('.account-menu-status');
    assert.equal(named(line()), 'Set a status Available');
    await click(line());
    assert.equal(dialog().querySelector('h2').textContent, 'Set a status');
    assert.equal(document.activeElement, dialog().querySelector('input[aria-label="Your own status"]'), 'focus starts in the text field');
    assert.equal(radio('Available').checked, true, 'Available is the default');
    assert.equal(dialog().querySelector('.status-dialog-after').disabled, true, 'nothing to clear while Available');
    await click(radio('In a meeting'));
    assert.equal(radio('In a meeting').checked, true);
    assert.match(named(dialog().querySelector('.status-dialog-after')), /Clears tonight at midnight/);
    await click(radio('1 hour'));
    assert.match(dialog().querySelector('.status-dialog-after .ui-help').textContent, /^Clears at .+, then you’re Available again\.$/);
    const save = [...dialog().querySelectorAll('button')].find(node => node.textContent === 'Save');
    const before = Date.now();
    await update(() => { save.click(); save.click(); });
    assert.equal(writes.length, 1, 'a double click sends once');
    assert.equal(writes[0].path, `/api/identity/workspaces/${w}/status`);
    const sent = writes[0].body.status;
    assert.deepEqual({ ...sent, clear_at: undefined }, { kind: 'meeting', emoji: null, text: null, clear_at: undefined });
    assert.ok(sent.clear_at >= before + 3_600_000 && sent.clear_at <= Date.now() + 3_600_000);
    await update(() => release());
    assert.equal(dialog(), null, 'saved and closed');
    assert.match(named(line()), /^In a meeting Clears .+ · change$/);
    assert.equal(document.activeElement, document.querySelector('button[aria-label="Your account"]'), 'focus returns to your photo');
  }, { answer: body => new Promise(resolve => { release = () => resolve(ok({ id: 'priya', online: true, last_seen: Date.now(), status: { ...body.status, set_at: Date.now() }, revision: 2 })); }) });
});

test('your own words with an emoji, a failed save that keeps them, and Clear status', async () => {
  let fail = true;
  const status = { kind: 'busy', emoji: null, text: null, clear_at: null, set_at: 1 };
  await shell(async ({ click, change, writes }) => {
    await click(document.querySelector('.account-menu-status'));
    assert.equal(radio('Busy').checked, true, 'opens on the current status');
    assert.equal(radio('Never').checked, true);
    const text = dialog().querySelector('input[aria-label="Your own status"]');
    await change(text, 'Out on deliveries');
    assert.equal([...dialog().querySelectorAll('.status-dialog-choices input')].some(input => input.checked), false, 'typing replaces the presets');
    await click(dialog().querySelector('button[aria-label="Choose an emoji"]'));
    await click([...dialog().querySelectorAll('.emoji-grid button')].find(node => node.textContent === '🚚'));
    assert.equal(dialog().querySelector('.composer-emoji').getAttribute('aria-label'), 'Choose an emoji, now 🚚');
    const save = [...dialog().querySelectorAll('button')].find(node => node.textContent === 'Save');
    await click(save);
    assert.deepEqual(writes.at(-1).body.status, { kind: 'custom', emoji: '🚚', text: 'Out on deliveries', clear_at: null });
    assert.match(dialog().querySelector('.ui-status').textContent, /Keep your status to 80 characters/);
    assert.equal(text.value, 'Out on deliveries', 'the draft stays after a failure');
    fail = false;
    const clear = [...dialog().querySelectorAll('button')].find(node => node.textContent === 'Clear status');
    await click(clear);
    assert.deepEqual(writes.at(-1).body, { status: null });
    assert.equal(dialog(), null);
    assert.equal(named(document.querySelector('.account-menu-status')), 'Set a status Available');
  }, { presence: [{ id: 'priya', online: true, last_seen: Date.now(), status, revision: 1 }],
    answer: body => fail ? { ok: false, status: 400, json: async () => ({ error: 'Keep your status to 80 characters' }) }
      : ok({ id: 'priya', online: true, last_seen: Date.now(), status: body.status, revision: 3 }) });
});

test('Show when I’m online is one switch in Preferences that saves at once', async () => {
  const writes = [];
  await reactFixture(async ({ render, click }) => {
    await (await import('../src/workspace-context.ts')).enterWorkspace();
    const { Preferences } = await import('../src/react/settings/personal-settings.tsx');
    await render(Preferences);
    const toggle = document.querySelector('.settings-presence input[role=switch]');
    assert.equal(document.querySelector('.settings-presence strong').textContent, 'Show when I’m online');
    assert.equal(document.querySelector('.settings-presence small').textContent, 'Off: nobody sees a green dot or “last seen” for you. Your status still shows.');
    assert.equal(toggle.closest('label').textContent, 'Show when I’m online', 'the switch is named');
    assert.equal(toggle.checked, true);
    await click(toggle);
    assert.equal(writes.length, 1);
    assert.deepEqual(writes[0].body, { desktop_alerts: true, channel_level: 'mentions', approval_alerts: true, show_online: false });
    assert.equal(document.querySelector('.settings-presence input[role=switch]').checked, false);
    assert.equal([...document.querySelectorAll('[role=status]')].some(node => node.textContent === 'Saved.'), true);
  }, { url: `http://localhost/w/${w}/?view=settings&section=preferences`, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return ok({ user, workspaces: [{ id: w, name: 'Lotus Interiors', role: 'Admin' }] });
    if (options.method === 'POST') { const body = JSON.parse(options.body); writes.push({ path, body }); return ok(body); }
    return ok({ desktop_alerts: true, channel_level: 'mentions', approval_alerts: true, show_online: true });
  } });
});
