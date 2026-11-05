import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

const w = '11111111-1111-4111-8111-111111111111';
const base = `http://localhost/w/${w}/?view=apps`;
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const session = role => response({ user: { id: 'me', email: 'me@test' }, workspaces: [{ id: w, name: 'Mehta', role }] });
const leave = { id: 'b1', kind: 'built', name: 'Leave requests', description: 'Ask for time off', audience: 'everyone', version: null, draft_revision: 1,
  builder_name: 'Me', is_builder: true, can_open: false, can_manage: true, people: [], versions: [] };
const starter = [{ path: 'cuckoo-app.json', size: 10, content: '{"name":"Leave requests","roles":["user"]}' }, { path: 'index.html', size: 20, content: '<h1>Leave</h1>' }];
const view = built => ({ apps: [], systems: [], built, can_manage: false, requests_waiting: 0 });

test('A member builds an app: one start per click burst, then it shows as their draft', async () => {
  const created = [];
  await reactFixture(async ({ render, click, change, update, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { AppsPage } = await import('../src/react/apps/apps-page.tsx');
    await render(AppsPage, { href: base });
    assert.match(document.querySelector('.ui-empty').textContent, /No apps yet/);
    await click(button('Build an app'));
    assert.equal(button('Start building').disabled, true, 'needs a name');
    await change(document.querySelector('[aria-label="Name"]'), 'Leave requests');
    await update(() => { button('Start building').click(); button('Start building').click(); });
    assert.equal(created.length, 1);
    assert.equal(created[0].name, 'Leave requests'); assert.equal(created[0].audience, 'everyone');
    assert.match(created[0].id, /^[0-9a-f-]{36}$/);
    assert.deepEqual([...document.querySelectorAll('.app-name')].map(n => n.textContent), ['Leave requests']);
    assert.equal(document.querySelector('.apps-filters [aria-pressed="true"]').textContent, 'Built here');
    assert.match(document.querySelector('.app-status').textContent, /Draft/);
    const panel = document.querySelector('.app-panel');
    assert.match(panel.textContent, /Not published yet\. Only its builder and admins see it\./);
    assert.equal(panel.querySelector('a.primary'), null, 'nothing to open before publishing');
    assert.match(panel.querySelector('a[href*="preview=1"]').getAttribute('href'), /run=[0-9a-f-]+&preview=1$/);
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return session('Member');
    if (path.endsWith('/chat-directory')) return response({ users: [] });
    if (path.endsWith('/apps/built')) { created.push(JSON.parse(options.body)); return response({ id: 'b1' }); }
    return response(view(created.length ? [{ ...leave, id: created[0].id }] : []));
  } });
});

test('The builder edits and publishes: a stale save keeps the draft, failing checks block, success goes live', async () => {
  const writes = [], publishes = [];
  let revision = 1, version = null, conflict = true;
  await reactFixture(async ({ render, click, change, button }) => {
    globalThis.confirm = () => true;
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { AppsPage } = await import('../src/react/apps/apps-page.tsx');
    await render(AppsPage, { href: base });
    await click(button('Code and publish'));
    const dialog = () => document.querySelector('dialog[open]');
    assert.deepEqual([...dialog().querySelectorAll('.built-files li button')].map(b => b.textContent), ['cuckoo-app.json', 'index.html']);
    assert.match(dialog().textContent, /Every check passes/);
    const editor = () => dialog().querySelector('[aria-label="Contents of index.html"]');
    assert.equal(editor().value, '<h1>Leave</h1>');
    await change(editor(), '<h1>Time off</h1>');
    assert.equal(button('Publish').disabled, true, 'saved draft only');
    // The first save meets a newer draft: the edit stays for another try.
    await click(button('Save file'));
    assert.match(dialog().textContent, /This item changed/);
    assert.equal(editor().value, '<h1>Time off</h1>');
    await click(button('Save file'));
    assert.deepEqual(writes.at(-1), { path: 'index.html', content: '<h1>Time off</h1>', revision: 1 });
    assert.match(dialog().textContent, /Saved index\.html\./);
    // The server's checks block a release and say why.
    await change(dialog().querySelector('[aria-label="What changed"]'), 'First');
    await click(button('Publish'));
    assert.match(dialog().querySelector('.built-check-failed').textContent, /secrets.*keys belong in Cuckoo/);
    assert.match(dialog().textContent, /Fix what the checks found/);
    await click(button('Publish'));
    assert.deepEqual(publishes.at(-1), { note: 'First', revision: 2 });
    assert.match(dialog().textContent, /Published version 1\./);
    assert.match(dialog().querySelector('.built-versions').textContent, /Version 1 · live/);
    assert.match(dialog().querySelector('a[download]').getAttribute('href'), /\/apps\/built\/b1\/download\?version=1$/);
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return session('Member');
    if (path.includes('/files?version=0')) return response({ files: writes.length ? [starter[0], { ...starter[1], content: '<h1>Time off</h1>' }] : starter, draft_revision: revision, checks: [] });
    if (path.endsWith('/files')) {
      if (conflict) { conflict = false; return response({ error: 'This item changed. Reload the saved version before trying again; your draft is preserved.' }, 409); }
      writes.push(JSON.parse(options.body)); revision++; return response({ draft_revision: revision });
    }
    if (path.endsWith('/publish')) {
      publishes.push(JSON.parse(options.body));
      if (publishes.length === 1) return response({ state: 'checks_failed', checks: [{ check: 'secrets', message: 'app.js looks like it contains an API token; keys belong in Cuckoo, never in code' }] });
      version = 1; return response({ state: 'published', version: 1 });
    }
    return response(view([{ ...leave, version, draft_revision: revision, versions: version ? [{ version: 1, note: 'First', at: 1_700_000_000 }] : [] }]));
  } });
});

test('People open a published app in a frame walled off from Cuckoo, and see why when they cannot', async () => {
  const published = { ...leave, version: 2, is_builder: false, can_open: true, can_manage: false };
  for (const [label, built] of [['opens', [published]], ['denied', []], ['draft', [{ ...published, version: null, can_open: false }]]]) {
    await reactFixture(async ({ render }) => {
      const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
      const { AppsPage } = await import('../src/react/apps/apps-page.tsx');
      await render(AppsPage, { href: `${base}&run=b1` });
      const frame = document.querySelector('iframe');
      if (label === 'opens') {
        assert.equal(frame.getAttribute('sandbox'), 'allow-scripts allow-forms');
        assert.doesNotMatch(frame.getAttribute('sandbox'), /same-origin/);
        assert.equal(frame.getAttribute('src'), `/api/identity/workspaces/${w}/apps/built/b1/run`);
        assert.match(document.querySelector('.apps-runner-bar').textContent, /Leave requests.*Version 2/);
      } else {
        assert.equal(frame, null, label);
        assert.match(document.querySelector('[role="alert"]').textContent, /isn't published for you/);
      }
      assert.match(document.querySelector('.apps-runner-bar a').getAttribute('href'), /view=apps$/);
    }, { url: `${base}&run=b1`, fetch: async path => path.endsWith('/session') ? session('Member') : response(view(built)) });
  }
});

test('Someone else sees a published app to open, without its code or versions', async () => {
  await reactFixture(async ({ render }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { AppsPage } = await import('../src/react/apps/apps-page.tsx');
    await render(AppsPage, { href: base });
    const panel = document.querySelector('.app-panel');
    const open = panel.querySelector('a.primary');
    assert.equal(open.textContent, 'Open Leave requests');
    assert.match(open.getAttribute('href'), /view=apps&run=b1$/);
    assert.match(panel.textContent, /Built here by Ravi/);
    assert.equal([...panel.querySelectorAll('button')].some(b => /Code|Settings|Remove/.test(b.textContent)), false);
  }, { url: base, fetch: async path => path.endsWith('/session') ? session('Member')
    : response(view([{ ...leave, version: 1, builder_name: 'Ravi', is_builder: false, can_open: true, can_manage: false, people: null, versions: null }])) });
});
