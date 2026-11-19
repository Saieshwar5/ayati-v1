import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement, useState } from 'react';
import { reactFixture } from './react-fixture.mjs';

// Drafts (schema 129): a draft is asked for once and written into the message box for you
// alone, where you edit, send or discard it. What your assistant knows about you is on its
// agent page (assistant-page.test.mjs).
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

test('a draft is asked for once with what you typed, written into the message box for you, and discarded on request', async () => {
  const writes = [];
  await reactFixture(async ({ render, update, dom }) => {
    // The hook reads the draft on a timer: run it by hand.
    let tick; dom.window.setInterval = fn => { tick = fn; return 1; }; dom.window.clearInterval = () => {};
    await (await import('../src/workspace-context.ts')).enterWorkspace();
    const model = await import('../src/react/chat/assistant-threads-model.ts');
    const { useAssistantDraft } = await import('../src/react/chat/assistant-draft.tsx');
    let box = ' say friday works ';
    function Box({ chat }) {
      const [value, setValue] = useState(box);
      const assist = useAssistantDraft({ chat, name: 'Ayati', value, change: text => { box = text; setValue(text); }, sent: {} });
      return createElement('div', null, assist.notice, assist.tool);
    }
    const screen = chat => () => createElement(model.AssistantThreads.Provider, { value: { threads: [], reload() {}, assistant: 'ayati', room: 'room' } }, createElement(Box, { chat }));
    const ask = () => document.querySelector('button[aria-label="Draft with Ayati"]');
    await render(screen('room'));
    assert.equal(ask(), null, 'not in your assistant’s own chat');
    await render(screen('dm-priya'));
    assert.match(ask().title, /Nothing is sent/);
    await update(() => { ask().click(); ask().click(); });
    assert.equal(writes.length, 1, 'one request despite a double click');
    assert.equal(writes[0].path, '/assistant/drafts');
    assert.deepEqual({ ...writes[0].body, id: undefined }, { id: undefined, chat: 'dm-priya', thread: null, instruction: 'say friday works' });
    assert.match(document.querySelector('.composer-notice').textContent, /Ayati is writing a draft.*Only you see this/);
    assert.equal(box, ' say friday works ', 'your words stay while it writes');
    assert.equal(ask().disabled, true);
    await update(() => { void tick(); });
    assert.equal(box, 'Friday at 3 works for me.', 'the draft is in the message box, for you to edit and send');
    assert.match(document.querySelector('.composer-notice').textContent, /Draft from Ayati.*Edit it, then send/);
    assert.equal(writes.length, 1, 'nothing is sent by writing it');
    await update(() => button(document, 'Discard').click());
    assert.equal(box, ' say friday works ', 'discarding puts your words back');
    assert.equal(document.querySelector('.composer-notice'), null);
    assert.equal(ask().disabled, false, 'ready for another');
  }, { url, fetch: server(writes) });
});
