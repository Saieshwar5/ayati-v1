import test from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace = '00000000-0000-4000-8000-000000000001';
const url = `http://localhost/w/${workspace}/?view=chat`;
const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });
const session = { user: { id: 'o', email: 'o@example.test' }, workspaces: [{ id: workspace, name: 'W', role: 'Member' }] };
const file = (id, more = {}) => ({ id, name: `${id}.pdf`, mime: 'application/pdf', size: 2048, at: 1_700_000_000, chat: 'c1', chat_name: 'You and Nova', from: 'Priya', made_by: null, library_item: null, ...more });

test('a conversation lists its files; one is saved to the Library once, keeping its identity after a failure', async () => {
  const kept = [];
  let fail = true;
  let files = [file('report', { made_by: 'Nova' }), file('invoice'), file('draft', { library_item: 'lib-1' })];
  await reactFixture(async ({ render, click }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { ConversationFiles } = await import('../src/react/chat/conversation-files.tsx');
    await render(ConversationFiles, { chat: 'c1' });
    assert.equal(document.querySelector('.conversation-file'), null, 'nothing is read until Files is opened');
    await click([...document.querySelectorAll('button')].find(b => b.textContent === 'Files'));
    const rows = [...document.querySelectorAll('.conversation-file')];
    assert.equal(rows.length, 3);
    assert.match(rows[0].textContent, /report\.pdf.*Made by Nova/);
    assert.match(rows[1].textContent, /invoice\.pdf.*From Priya/);
    assert.equal(rows[2].querySelector('a.conversation-file-kept').textContent, 'In Library');
    assert.equal(rows[2].querySelector('button'), null);
    assert.ok(rows[0].querySelector('a').getAttribute('href').endsWith('/objects/report'));
    await click(rows[0].querySelector('button'));
    assert.match(rows[0].querySelector('[role="alert"]').textContent, /Storage full/);
    fail = false;
    await click(document.querySelector('.conversation-file button'));
    assert.equal(kept.length, 2);
    assert.equal(kept[0].id, kept[1].id, 'the same Library item on retry');
    assert.equal(document.querySelectorAll('a.conversation-file-kept').length, 2);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (options.body) {
      kept.push(JSON.parse(options.body));
      if (fail) return response({ error: 'Storage full' }, 507);
      files = files.map(f => f.id === 'report' ? { ...f, library_item: kept.at(-1).id } : f);
      return response({ id: kept.at(-1).id });
    }
    return response({ files });
  } });
});

test("an agent's files are grouped by conversation; a keeper sees other conversations' names only", async () => {
  await reactFixture(async ({ render }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { AgentArtifacts } = await import('../src/react/chat/conversation-files.tsx');
    await render(AgentArtifacts, { agent: 'nova', name: 'Nova' });
    const groups = [...document.querySelectorAll('section')].map(s => s.querySelector('h4').textContent);
    assert.deepEqual(groups, ['You and Nova', 'Ravi and Nova · not your conversation']);
    const theirs = document.querySelectorAll('section')[1];
    assert.equal(theirs.querySelector('a, button'), null, 'no download and no save for a conversation they are not in');
    assert.match(document.body.textContent, /You look after Nova/);
  }, { url, fetch: async path => path.endsWith('/session') ? response(session) : response({ keeper: true, files: [
    file('report', { mine: true }), file('payments', { chat: 'c2', chat_name: 'Ravi and Nova', mine: false, from: 'Ravi' }),
  ] }) });
});
