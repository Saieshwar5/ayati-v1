import { registerHooks } from 'node:module';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

registerHooks({ load(url, context, next) {
  if (url.endsWith('.css')) return { format: 'module', source: 'export {};', shortCircuit: true };
  return next(url, context);
} });

const workspace = '11111111-1111-4111-8111-111111111111';
const base = `/w/${workspace}/`;
const session = { user: { id: 'owner', email: 'owner@example.com' }, workspaces: [{ id: workspace, name: 'Store', role: 'Owner' }] };
const profiles = ['local-agent', 'assistant-two'].map((id, i) => ({ id, name: i ? 'Ramesh' : 'Relay', state: 'ready', editable: false, revision: 1, role: '', model: 'test', memory: false, browser: false, browser_available: false }));
const directory = { agents: profiles.map(profile => ({ profile, activity: { pending_approvals: 0, pending_tasks: 0 } })), creation_available: true, limit: 4 };
const response = data => ({ ok: true, status: 200, json: async () => data });
function dataFor(path) {
  if (path.endsWith('/session')) return session;
  if (path.endsWith('/agents')) return directory;
  if (path.endsWith('/home')) return { agents: [] };
  if (path.endsWith('/panels')) return { panels: [], available: true };
  if (path.endsWith('/apps')) return { apps: [], available: true };
  if (path.endsWith('/home/preferences')) return { preferences: null };
  if (path.endsWith('/people')) return { role: 'Owner', members: [{ id: 'owner', email: 'owner@example.com', role: 'Owner' }], invitations: [] };
  if (path.endsWith('/health')) return { model_ready: true };
  if (path.endsWith('/browser/status')) return { configured: false, owner: false };
  if (path.endsWith('/memory?query=')) return { enabled: false, memories: [] };
  if (path.endsWith('/connections')) return { connections: [], available: false };
  if (path.endsWith('/state')) return { messages: [], tasks: [], artifacts: [] };
  if (path.endsWith('/setup')) return { checks: [], supported: true };
  throw new Error(`Unexpected request: ${path}`);
}
async function prepare(dom) {
  const original = Object.fromEntries(['Event', 'EventSource', 'Option', 'FormData', 'sessionStorage', 'matchMedia', 'requestAnimationFrame', 'cancelAnimationFrame'].map(key => [key, globalThis[key]]));
  const timers = new Map(); let timer = 0;
  dom.window.setInterval = handler => { timers.set(++timer, handler); return timer; };
  dom.window.clearInterval = id => timers.delete(id);
  const streams = new Set();
  globalThis.Event = dom.window.Event;
  globalThis.Option = dom.window.Option;
  globalThis.FormData = dom.window.FormData;
  globalThis.sessionStorage = dom.window.sessionStorage;
  globalThis.requestAnimationFrame = handler => { handler(); return 0; };
  globalThis.cancelAnimationFrame = () => {};
  globalThis.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  globalThis.EventSource = class { constructor(url) { this.url = url; streams.add(this); } close() { streams.delete(this); } };
  const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
  const { WorkspaceApplication } = await import('../src/react/workspace-application.tsx');
  return { WorkspaceApplication, timers, streams, async cleanup() {
    const { disposeViewLifetime } = await import('../src/view-lifetime.ts'); disposeViewLifetime();
    Object.assign(globalThis, original);
  } };
}
async function until(update, condition) {
  for (let i = 0; i < 80 && !condition(); i++) await update();
  assert.ok(condition(), 'Expected UI did not appear: '+document.body.textContent);
}

test('sidebar navigation retains the shell, handles rapid changes and cleans feature timers', async () => {
  let releaseHome; let firstHome = true;
  await reactFixture(async ({ dom, render, click, update }) => {
    const setup = await prepare(dom);
    try {
      await render(setup.WorkspaceApplication);
      await until(update, () => document.querySelector('.home'));
      const rail = document.querySelector('.workspace-rail');
      for (const [label, selector] of [['Apps', '.apps-page'], ['Users', '[aria-label=Users]'], ['Settings', '.settings-page'], ['Home', '.home']]) {
        const link = [...rail.querySelectorAll('a')].find(node => node.title === label);
        await click(link);
        assert.equal(document.querySelector('.workspace-rail'), rail);
        await until(update, () => document.querySelector(selector));
        if (label === 'Apps') await update(() => releaseHome?.(response({ agents: [] })));
      }
      // Two clicks before feature imports complete must leave only the last route mounted.
      await update(() => {
        rail.querySelector('[title=Apps]').click();
        rail.querySelector('[title=Settings]').click();
      });
      await until(update, () => document.querySelector('.settings-page'));
      assert.equal(document.querySelector('.apps-page'), null);
      assert.equal(setup.timers.size, 0, 'Old Home/Apps pollers must be removed');
      await update(() => dom.window.history.back());
      await until(update, () => document.querySelector('.apps-page'));
      assert.equal(document.querySelector('.workspace-rail'), rail);
      assert.equal(document.querySelector('[title=Apps]').getAttribute('aria-current'), 'page');
    } finally { await setup.cleanup(); }
  }, { url: `http://localhost${base}?view=home`, fetch: async path => {
    if (path.endsWith('/home') && firstHome) { firstHome = false; return new Promise(resolve => { releaseHome = resolve; }); }
    return response(dataFor(path));
  } });
});

test('agent switching retains contacts, isolates drafts and ignores late agent responses', async () => {
  let releaseOld;
  let initialState = true;
  const requests = [];
  await reactFixture(async ({ dom, render, click, update }) => {
    const setup = await prepare(dom);
    try {
      await render(setup.WorkspaceApplication);
      await until(update, () => document.querySelector('#input') && document.querySelector('.agent-link'));
      const rail = document.querySelector('.workspace-rail');
      const contacts = document.querySelector('.persistent-contacts');
      document.querySelector('#conversation').scrollTop = 123;
      const input = document.querySelector('#input'); input.value = 'Draft for Relay'; input.dispatchEvent(new dom.window.Event('input'));
      await click([...contacts.querySelectorAll('a')].find(a => a.textContent === 'Ramesh'));
      await until(update, () => document.querySelector('#input')?.getAttribute('aria-label') === 'Message Ramesh');
      assert.equal(document.querySelector('.persistent-contacts'), contacts);
      assert.equal(document.querySelector('.workspace-rail'), rail);
      assert.equal(document.querySelector('#input').value, '');
      assert.equal(setup.streams.size, 1, 'Previous agent stream must close');
      await update(() => releaseOld?.(response({ messages: [{ id: 'old', role: 'assistant', content: 'STALE RELAY REPLY', state: 'done' }], tasks: [], artifacts: [] })));
      assert.doesNotMatch(document.querySelector('#conversation').textContent, /STALE/);
      document.querySelector('#input').value = 'Draft for Ramesh'; document.querySelector('#input').dispatchEvent(new dom.window.Event('input'));
      await click([...contacts.querySelectorAll('a')].find(a => a.textContent === 'Relay'));
      await until(update, () => document.querySelector('#input')?.getAttribute('aria-label') === 'Message Relay');
      assert.equal(document.querySelector('#input').value, 'Draft for Relay');
      assert.equal(document.querySelector('#conversation').scrollTop, 123);
      assert.ok(requests.some(request => request.path.endsWith('/state') && request.agent === 'assistant-two'));
      assert.equal(setup.streams.size, 1);
    } finally { await setup.cleanup(); assert.equal(setup.streams.size, 0); }
  }, { url: `http://localhost${base}?agent=local-agent`, fetch: async (path, options = {}) => {
    requests.push({ path, agent: options.headers?.get?.('X-Ayati-Agent') });
    if (path.endsWith('/state') && initialState) { initialState = false; return new Promise(resolve => { releaseOld = resolve; }); }
    return response(dataFor(path));
  } });
});

test('navigation interception leaves workspace switches, downloads and other destinations native', async () => {
  await reactFixture(async ({ dom }) => {
    const { localDestination, workspacePage } = await import('../src/navigation.ts');
    const current = new URL(dom.window.location.href);
    assert.equal(workspacePage(new URL('?agent=assistant-two', current)), 'chat');
    assert.equal(workspacePage(new URL('?view=users', current)), 'people');
    assert.ok(localDestination('?view=apps', current));
    for (const path of ['/w/22222222-2222-4222-8222-222222222222/', '/auth/login', '/api/identity/session', 'https://example.com/']) assert.equal(localDestination(path, current), undefined);
  }, { url: `http://localhost${base}?view=home` });
});


test('shared chat selection retains its sidebar and rejects messages from an old selection', async () => {
  let releaseOld;
  let initial = true;
  await reactFixture(async ({ dom, render, click, update }) => {
    const setup = await prepare(dom);
    try {
      await render(setup.WorkspaceApplication);
      await until(update, () => [...document.querySelectorAll('.shared-contact')].some(a => a.textContent === 'Second'));
      const sidebar = document.querySelector('.shared-sidebar');
      const rail = document.querySelector('.workspace-rail');
      await click([...sidebar.querySelectorAll('a')].find(a => a.textContent === 'Second'));
      await update(() => releaseOld?.(response({ messages: [{ id: 'old', author: 'owner', name: 'Owner', kind: 'user', content: 'OLD SELECTION', state: 'complete', at: 1 }], agents: [] })));
      await until(update, () => document.querySelector('.shared-heading h2')?.textContent === 'Second');
      assert.equal(document.querySelector('.shared-sidebar'), sidebar);
      assert.equal(document.querySelector('.workspace-rail'), rail);
      assert.doesNotMatch(document.querySelector('.shared-messages').textContent, /OLD SELECTION/);
      const input = document.querySelector('.shared-composer textarea');
      input.value = 'Second draft'; input.dispatchEvent(new dom.window.Event('input'));
      await click([...sidebar.querySelectorAll('a')].find(a => a.textContent === 'First'));
      await until(update, () => document.querySelector('.shared-heading h2')?.textContent === 'First');
      assert.equal(input.value, '');
      await click([...sidebar.querySelectorAll('a')].find(a => a.textContent === 'Second'));
      await until(update, () => document.querySelector('.shared-heading h2')?.textContent === 'Second');
      assert.equal(input.value, 'Second draft');
    } finally { await setup.cleanup(); }
  }, { url: `http://localhost${base}?view=chat&conversation=first`, fetch: async path => {
    if (path.endsWith('/chat-directory')) return response({ users: [], agents: [], grants: [], role: 'Owner' });
    if (path.endsWith('/chats')) return response({ chats: [{ id: 'first', name: 'First' }, { id: 'second', name: 'Second' }] });
    if (path.includes('/chats/') && path.endsWith('/messages')) {
      if (path.includes('/first/') && initial) { initial = false; return new Promise(resolve => { releaseOld = resolve; }); }
      return response({ messages: [], agents: [] });
    }
    return response(dataFor(path));
  } });
});


test('creating a shared conversation closes and resets its dialog during in-place navigation', async () => {
  let created = false;
  await reactFixture(async ({ dom, render, click, update, button }) => {
    const setup = await prepare(dom);
    try {
      await render(setup.WorkspaceApplication);
      await until(update, () => document.querySelector('.shared-new'));
      await click(button('New chat'));
      const dialog = document.querySelector('.shared-new');
      dialog.querySelector('input').value = 'New team chat';
      await update(() => { void dialog.querySelector('form').onsubmit(new dom.window.Event('submit')); });
      await until(update, () => document.querySelector('.shared-heading h2')?.textContent === 'New team chat');
      assert.equal(dialog.open, false);
      assert.equal(dialog.querySelector('input').value, '');
      await click(button('New chat'));
      assert.equal(dialog.open, true);
      assert.equal(dialog.querySelector('button[type=submit],button:not([type])')?.disabled ?? false, false);
    } finally { await setup.cleanup(); }
  }, { url: `http://localhost${base}?view=chat`, fetch: async (path, options = {}) => {
    if (path.endsWith('/chat-directory')) return response({ users: [], agents: [], grants: [], role: 'Owner' });
    if (path.endsWith('/chats')) {
      if (options.method === 'POST') { created = true; return response({ id: 'created' }); }
      return response({ chats: created ? [{ id: 'created', name: 'New team chat' }] : [] });
    }
    if (path.endsWith('/messages')) return response({ messages: [], agents: [] });
    return response(dataFor(path));
  } });
});

test('Home separates outcomes from decisions and keeps customization behind its toolbar', async () => {
  let permissions = [];
  const task = { id: 'finished', status: 'completed', detail: 'Report ready', request: 'Daily report', created_at: '2026-10-20 10:00:00' };
  await reactFixture(async ({ dom, render, click, update, button }) => {
    const setup = await prepare(dom);
    try {
      await render(setup.WorkspaceApplication);
      await until(update, () => document.querySelector('#home-activity-count')?.textContent.includes('1 outcome'));
      assert.equal(document.querySelector('#home-agents'), null);
      assert.equal(document.querySelector('#home-decisions').hidden, true);
      assert.match(document.querySelector('.home-welcome').textContent, /A home for what matters/);
      await click(document.querySelector('#home-history-toggle'));
      await click(document.querySelector('#home-updates'));
      assert.equal(document.querySelector('.attention-inbox').open, true);
      assert.match(document.querySelector('#home-attention').textContent, /Daily report/);
      assert.match(document.querySelector('.inbox-filters [aria-pressed=true]').textContent, /Updates/);
      await click(button('Close inbox'));
      permissions = [{ id: 'approval', task_id: 'queued', title: 'Send supplier request', action: { operation: 'send' } }];
      await click(document.querySelector('#home-refresh'));
      assert.equal(document.querySelector('#home-decisions').hidden, false);
      await click(document.querySelector('#home-decisions'));
      assert.match(document.querySelector('.inbox-filters [aria-pressed=true]').textContent, /Decisions/);
      assert.match(document.querySelector('#home-attention').textContent, /Send supplier request/);
      await click(button('Close inbox'));
      await click(button('Customize Home'));
      assert.equal(document.querySelector('[data-arrange]').hidden, false);
      await click(document.querySelector('[data-arrange]'));
      assert.equal(document.querySelector('[data-arrange]').hidden, true);
      await click(document.querySelector('.home-toolbar-actions .primary'));
      await until(update, () => document.querySelector('.creation-chooser')?.open);
    } finally { await setup.cleanup(); }
  }, { url: `http://localhost${base}?view=home`, fetch: async path => {
    if (path.endsWith('/home')) return response({ agents: [{ profile: profiles[0], attention: { active: [], recent: [task], outcomes: [task], outcome_count: 1, permissions } }] });
    return response(dataFor(path));
  } });
});

test('Home keeps saved panels, arrangement order and content on a failed refresh', async () => {
  let offline = false;
  const panels = ['Sales', 'Stock'].map((title, i) => ({ id: `panel-${i}`, agent_id: 'local-agent', revision: 1,
    state: 'active', updates_enabled: false, source_executions: [], updated_at: '2026-10-20 10:00:00',
    active: { updated_at: '2026-10-20 10:00:00', definition: { title, purpose: 'Saved snapshot', template: 'metric', sections: [{ kind: 'metric', label: title, value: '42' }] } } }));
  await reactFixture(async ({ dom, render, click, update, button }) => {
    const setup = await prepare(dom);
    try {
      await render(setup.WorkspaceApplication);
      await until(update, () => document.querySelectorAll('[data-panel]').length === 2);
      assert.equal(document.querySelector('.home-welcome'), null);
      assert.equal(document.querySelector('.panel-arrange'), null);
      await click(button('Customize Home'));
      const later = [...document.querySelector('[data-panel="panel-0"]').querySelectorAll('button')].find(b => b.textContent === 'Move later');
      await click(later);
      assert.equal(document.querySelector('[data-panel]').dataset.panel, 'panel-1');
      await click(document.querySelector('[data-arrange]'));
      offline = true;
      await update(() => dom.window.dispatchEvent(new dom.window.Event('focus')));
      assert.match(document.querySelector('[data-panel-status]').textContent, /connection lost/i);
      assert.equal(document.querySelectorAll('[data-panel]').length, 2);
      assert.equal(document.querySelector('[data-panel]').dataset.panel, 'panel-1');
      assert.match(document.querySelector('[data-panel]').textContent, /42/);
    } finally { await setup.cleanup(); }
  }, { url: `http://localhost${base}?view=home`, fetch: async path => {
    if (path.endsWith('/panels')) { if (offline) throw Error('offline'); return response({ panels, available: true }); }
    return response(dataFor(path));
  } });
});
