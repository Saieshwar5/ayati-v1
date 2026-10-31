import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const workspace = '11111111-1111-4111-8111-111111111111';
const agent = { id: 'agent', name: 'Nova', role: 'Help', model: 'fixture', audience: 'selected',
  revision: 1, editable: true, settings: { memory: false, history: 'since_join' } };
const chats = [{ id: 'private-chat', name: 'Private delivery', agents: ['agent'] },
  { id: 'other-agent-chat', name: 'Other private chat', agents: ['other'] }];
const response = (value, status = 200) => ({ ok: status < 400, status, json: async () => value });
const options = fetch => ({ url: `http://localhost/w/${workspace}/?view=agents`, fetch });

test('a new note requires an explicit audience and retains a failed scoped draft without duplicate submission', async () => {
  const writes = []; let finish;
  await reactFixture(async ({ render, button, click, change, submit, update }) => {
    const { Knowledge } = await import('../src/react/agents/knowledge.tsx');
    await render(Knowledge, { agent });
    assert.match(document.body.textContent, /private chats are kept separate/);
    assert.match(document.body.textContent, /only messages since this agent joined/);
    assert.match(document.body.textContent, /Approved knowledge is off/);
    await click(button('Add approved note'));
    const audience = document.querySelector('select');
    assert.equal(audience.value, '');
    assert.equal(button('Save note').disabled, true);
    assert.deepEqual([...audience.options].map(option => option.value), ['', 'private-chat', 'agent']);
    await change(document.querySelector('input'), 'Private preference');
    await change(document.querySelector('textarea'), 'Only this conversation should use the note.');
    await submit(document.querySelector('form'));
    assert.equal(writes.length, 0);
    await change(audience, 'private-chat');
    await update(() => { document.querySelector('form').requestSubmit(); document.querySelector('form').requestSubmit(); });
    assert.equal(writes.length, 1);
    assert.equal(writes[0].chat, 'private-chat');
    await update(() => finish(response({ error: 'Save unavailable' }, 503)));
    assert.equal(document.querySelector('textarea').value, 'Only this conversation should use the note.');
    assert.equal(document.querySelector('select').value, 'private-chat');
    assert.match(document.querySelector('[role=status]').textContent, /Save unavailable/);
  }, options(async (path, request = {}) => {
    if (request.body) { writes.push(JSON.parse(request.body)); return new Promise(resolve => { finish = resolve; }); }
    return response(path.endsWith('/chats') ? { chats } : { notes: [] });
  }));
});

test('using a note in every conversation is a deliberate choice with its broader audience explained', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click, change, submit }) => {
    const { Knowledge } = await import('../src/react/agents/knowledge.tsx');
    await render(Knowledge, { agent });
    await click(button('Add approved note'));
    await change(document.querySelector('select'), 'agent');
    assert.match(document.body.textContent, /anyone who can talk to this agent/);
    assert.match(document.body.textContent, /Do not include private employee or customer details/);
    await change(document.querySelector('input'), 'Public company guidance');
    await change(document.querySelector('textarea'), 'Use approved branding.');
    await submit(document.querySelector('form'));
    assert.equal(writes.length, 1);
    assert.equal(writes[0].chat, null);
    assert.equal(writes[0].source_message, null);
  }, options(async (path, request = {}) => {
    if (request.body) writes.push(JSON.parse(request.body));
    return response(path.endsWith('/chats') ? { chats } : { notes: [] });
  }));
});

test('editing and forgetting preserve immutable scope while excluding server-only read metadata from writes', async () => {
  const note = { id: 'note', title: 'Delivery', content: 'Private note', chat: 'private-chat',
    source_message: 'source', revision: 4, forgotten: false };
  const stored = { ...note, workspace, agent: 'agent', creator_id: 'owner', created_at: '2026-10-26' };
  const writes = [];
  await reactFixture(async ({ render, button, click, change, submit }) => {
    const { Knowledge } = await import('../src/react/agents/knowledge.tsx');
    await render(Knowledge, { agent });
    await click(button('Edit'));
    assert.equal(document.querySelector('select').disabled, true);
    assert.equal(document.querySelector('select').value, note.chat);
    await change(document.querySelector('textarea'), 'Corrected private note');
    await submit(document.querySelector('form'));
    assert.deepEqual(writes, [{ ...note, content: 'Corrected private note' }]);
    await click(button('Forget'));
    assert.deepEqual(writes[1], { ...note, content: 'Corrected private note', revision: 5, forgotten: true });
  }, options(async (path, request = {}) => {
    if (request.body) {
      const input = JSON.parse(request.body); writes.push(input);
      Object.assign(stored, input, { revision: input.revision + 1 });
    }
    return response(path.endsWith('/chats') ? { chats } : { notes: stored.forgotten ? [] : [stored] });
  }));
});

test('read-only agent viewers see audience information without knowledge mutation controls', async () => {
  await reactFixture(async ({ render, button }) => {
    const { Knowledge } = await import('../src/react/agents/knowledge.tsx');
    await render(Knowledge, { agent: { ...agent, editable: false } });
    assert.equal(button('Add approved note'), undefined);
    assert.equal(button('Edit'), undefined);
    assert.equal(button('Forget'), undefined);
    assert.match(document.body.textContent, /Every conversation with this agent/);
  }, options(async path => response(path.endsWith('/chats') ? { chats } : {
    notes: [{ id: 'note', title: 'General guidance', content: 'Public note', chat: null }],
  })));
});
