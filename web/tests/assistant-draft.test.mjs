import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';

// Drafts and "about you" (schema 129): a draft is asked for once, shown only to
// you and goes into the message box when you use it; settings keep your words
// when saving fails.
const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=chat&conversation=dm-priya`;
const session = { user: { id: 'ravi', email: 'ravi@example.test' }, workspaces: [{ id: w, name: 'Northwind', role: 'Member' }] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
function server(writes, { draft = { state: 'complete', text: 'Friday at 3 works for me.' }, settings, fail = 0 } = {}) {
  return async (path, options = {}) => {
    const short = path.replace(`/api/identity/workspaces/${w}`, '');
    if (options.method === 'POST') {
      writes.push({ path: short, body: JSON.parse(options.body) });
      if (writes.length <= fail) return response({ error: 'That is too long to keep' }, 400);
      return response({ ok: true, reply: 'r1' });
    }
    if (path.endsWith('/session')) return response(session);
    if (short === '/assistant/drafts/r1') return response(draft);
    if (short === '/assistant/settings') return response(settings ?? { about: 'Head of sales.', style: '', checks: false, every_minutes: 240, checklist: '' });
    return response({ threads: [] });
  };
}
const button = (node, label) => [...node.querySelectorAll('button')].find(b => b.textContent.trim() === label);

test('a draft is asked for once with what you typed, shown to you, and used only when you choose', async () => {
  const writes = [], used = [];
  await reactFixture(async ({ render, update, dom }) => {
    // The component reads the draft on a timer: run it by hand.
    let tick; dom.window.setInterval = fn => { tick = fn; return 1; }; dom.window.clearInterval = () => {};
    await (await import('../src/workspace-context.ts')).enterWorkspace();
    const model = await import('../src/react/chat/assistant-threads-model.ts');
    const { AssistantDraft } = await import('../src/react/chat/assistant-draft.tsx');
    const screen = chat => () => createElement(model.AssistantThreads.Provider, { value: { threads: [], reload() {}, assistant: 'ayati', room: 'room' } },
      createElement(AssistantDraft, { chat, name: 'Ayati', instruction: ' say friday works ', use: text => used.push(text), sent: {} }));
    await render(screen('room'));
    assert.equal(document.querySelector('.chat-draft-ask'), null, 'not in your assistant’s own chat');
    await render(screen('dm-priya'));
    const ask = button(document, 'Draft with Ayati');
    await update(() => { ask.click(); ask.click(); });
    assert.equal(writes.length, 1, 'one request despite a double click');
    assert.equal(writes[0].path, '/assistant/drafts');
    assert.deepEqual({ ...writes[0].body, id: undefined }, { id: undefined, chat: 'dm-priya', thread: null, instruction: 'say friday works' });
    assert.match(document.querySelector('.chat-draft').textContent, /Ayati is writing a draft.*Only you see this/);
    await update(() => { void tick(); });
    assert.match(document.querySelector('.chat-draft').textContent, /Draft from Ayati.*Friday at 3 works for me\./);
    assert.deepEqual(used, [], 'nothing is used or sent by showing it');
    await update(() => button(document, 'Use draft').click());
    assert.deepEqual(used, ['Friday at 3 works for me.']);
    assert.ok(button(document, 'Draft with Ayati'), 'ready for another');
  }, { url, fetch: server(writes) });
});

test('what your assistant knows about you is yours to edit; a failed save keeps your words', async () => {
  const writes = [];
  await reactFixture(async ({ render, click, change }) => {
    await (await import('../src/workspace-context.ts')).enterWorkspace();
    const { AssistantSettings } = await import('../src/react/chat/assistant-settings.tsx');
    await render(() => createElement(AssistantSettings, { name: 'Ayati', close() {} }));
    const dialog = document.querySelector('dialog'), areas = () => [...dialog.querySelectorAll('textarea')];
    assert.equal(areas()[0].value, 'Head of sales.');
    assert.equal(areas().length, 2, 'the checklist shows only with checks on');
    await change(areas()[1], 'Short and friendly.');
    await click(dialog.querySelector('input[type=checkbox]'));
    assert.equal(areas().length, 3);
    await change(dialog.querySelector('select'), '60');
    await click(button(dialog, 'Save'));
    assert.match(dialog.textContent, /too long to keep/);
    assert.equal(areas()[1].value, 'Short and friendly.');
    await click(button(dialog, 'Save'));
    assert.deepEqual(writes[1], { path: '/assistant/settings', body: { about: 'Head of sales.', style: 'Short and friendly.', checks: true, every_minutes: 60, checklist: '' } });
  }, { url, fetch: server(writes, { fail: 1 }) });
});
