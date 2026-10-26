import { act } from 'react';
import { outboxStorage } from './outbox-storage-fixture.mjs';
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
  if (path.endsWith('/chat-directory')) return { users: [], agents: profiles.map(p => ({ id:p.id, name:p.name })), role: 'Owner' };
  if (path.endsWith('/message-capabilities')) return { live: true };
  if (path.endsWith('/unread')) return { unread: [] };
  if (path.endsWith('/chats')) return { chats: [] };
  if (path.endsWith('/home')) return { agents: [] };
  // These legacy Home journeys exercise the explicit private-runtime adapter.
  if (path.includes('/attention?')) return { available: false, items: [], next_before: null };
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
  const original = Object.fromEntries(['indexedDB', 'Event', 'EventSource', 'Option', 'FormData', 'sessionStorage', 'matchMedia', 'requestAnimationFrame', 'cancelAnimationFrame'].map(key => [key, globalThis[key]]));
  const timers = new Map(); let timer = 0;
  dom.window.setInterval = handler => { timers.set(++timer, handler); return timer; };
  dom.window.clearInterval = id => timers.delete(id);
  const streams = new Set();
  globalThis.indexedDB = outboxStorage();
  globalThis.Event = dom.window.Event;
  globalThis.Option = dom.window.Option;
  globalThis.FormData = dom.window.FormData;
  globalThis.sessionStorage = dom.window.sessionStorage;
  globalThis.requestAnimationFrame = handler => { handler(); return 0; };
  globalThis.cancelAnimationFrame = () => {};
  globalThis.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  globalThis.EventSource = class { constructor(url) { this.url = url; if (!url.includes('message-events')) streams.add(this); } addEventListener() {} close() { streams.delete(this); } };
  const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
  const { WorkspaceApplication } = await import('../src/react/workspace-application.tsx');
  return { WorkspaceApplication, timers, streams, async cleanup() {
    const { disposeViewLifetime } = await import('../src/view-lifetime.ts'); await act(async () => { disposeViewLifetime(); await Promise.resolve(); });
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
      for (const [label, selector] of [['Apps', '.apps-page'], ['Settings', '.settings-page'], ['Home', '.home']]) {
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
      assert.equal(setup.timers.size, 1, 'Only the workspace outbox/receipt timer remains');
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
  await reactFixture(async ({ dom, render, click, update, change }) => {
    const setup = await prepare(dom);
    try {
      await render(setup.WorkspaceApplication);
      await until(update, () => document.querySelector('.message-composer textarea') && document.querySelector('.chat-contact'));
      const rail = document.querySelector('.workspace-rail');
      const contacts = document.querySelector('.chat-directory');
      document.querySelector('#conversation').scrollTop = 123;
      await change(document.querySelector('.message-composer textarea'), 'Draft for Relay');
      await click([...contacts.querySelectorAll('a')].find(a => a.textContent.includes('Ramesh')));
      await until(update, () => document.querySelector('.message-composer textarea')?.getAttribute('aria-label') === 'Message Ramesh');
      assert.equal(document.querySelector('.chat-directory'), contacts);
      assert.equal(document.querySelector('.workspace-rail'), rail);
      assert.equal(document.querySelector('.message-composer textarea').value, '');
      assert.equal(setup.streams.size, 1, 'Previous agent stream must close');
      await update(() => releaseOld?.(response({ messages: [{ id: 'old', role: 'assistant', content: 'STALE RELAY REPLY', state: 'done' }], tasks: [], artifacts: [] })));
      assert.doesNotMatch(document.querySelector('#conversation').textContent, /STALE/);
      await change(document.querySelector('.message-composer textarea'), 'Draft for Ramesh');
      await click([...contacts.querySelectorAll('a')].find(a => a.textContent.includes('Cuckoo')));
      await until(update, () => document.querySelector('.message-composer textarea')?.getAttribute('aria-label') === 'Message Cuckoo');
      assert.equal(document.querySelector('.message-composer textarea').value, 'Draft for Relay');
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
    assert.equal(workspacePage(new URL('?view=users', current)), 'settings');
    assert.ok(localDestination('?view=apps', current));
    for (const path of ['/w/22222222-2222-4222-8222-222222222222/', '/auth/login', '/api/identity/session', 'https://example.com/']) assert.equal(localDestination(path, current), undefined);
  }, { url: `http://localhost${base}?view=home` });
});


test('shared chat selection retains its sidebar and rejects messages from an old selection', async () => {
  let releaseOld;
  let initial = true;
  await reactFixture(async ({ dom, render, click, update, change }) => {
    const setup = await prepare(dom);
    try {
      await render(setup.WorkspaceApplication);
      await until(update, () => [...document.querySelectorAll('.chat-contact')].some(a => a.textContent.includes('Second')));
      const sidebar = document.querySelector('.chat-directory');
      const rail = document.querySelector('.workspace-rail');
      await click([...sidebar.querySelectorAll('a')].find(a => a.textContent.includes('Second')));
      await update(() => releaseOld?.(response({ messages: [{ id: 'old', author: 'owner', name: 'Owner', kind: 'user', content: 'OLD SELECTION', state: 'complete', at: 1 }], agents: [] })));
      await until(update, () => document.querySelector('.chat-pane-header h2')?.textContent === 'Second');
      assert.equal(document.querySelector('.chat-directory'), sidebar);
      assert.equal(document.querySelector('.workspace-rail'), rail);
      assert.doesNotMatch(document.querySelector('.chat-message-list').textContent, /OLD SELECTION/);
      const input = document.querySelector('.message-composer textarea');
      await change(input, 'Second draft');
      await click([...sidebar.querySelectorAll('a')].find(a => a.textContent.includes('First')));
      await until(update, () => document.querySelector('.chat-pane-header h2')?.textContent === 'First');
      assert.equal(document.querySelector('.message-composer textarea').value, '');
      await click([...sidebar.querySelectorAll('a')].find(a => a.textContent.includes('Second')));
      await until(update, () => document.querySelector('.chat-pane-header h2')?.textContent === 'Second');
      assert.equal(document.querySelector('.message-composer textarea').value, 'Second draft');
    } finally { await setup.cleanup(); }
  }, { url: `http://localhost${base}?view=chat&conversation=first`, fetch: async path => {
    if (path.endsWith('/chat-directory')) return response({ users: [], agents: [], grants: [], role: 'Owner' });
    if (path.endsWith('/chats')) return response({ chats: [{ id: 'first', name: 'First', kind:'channel', description:'', users:['owner'], agents:[] }, { id: 'second', name: 'Second', kind:'channel', description:'', users:['owner'], agents:[] }] });
    if (path.includes('/chats/') && path.endsWith('/messages')) {
      if (path.includes('/first/') && initial) { initial = false; return new Promise(resolve => { releaseOld = resolve; }); }
      return response({ messages: [], agents: [] });
    }
    return response(dataFor(path));
  } });
});


test('creating a channel validates membership and resets its dialog', async () => {
  let created = false;
  await reactFixture(async ({ dom, render, click, update, button, change, submit }) => {
    const setup = await prepare(dom);
    try {
      await render(setup.WorkspaceApplication);
      await until(update, () => button('Create channel') && !button('Create channel').disabled);
      await click(button('Create channel'));
      const dialog = document.querySelector('dialog');
      await change(dialog.querySelector('input'), 'New team chat');
      assert.equal(dialog.querySelector('button[type=submit]').disabled, true);
      await click(dialog.querySelector('input[type=checkbox]'));
      await submit(dialog.querySelector('form'));
      await until(update, () => document.querySelector('.chat-pane-header h2')?.textContent === 'New team chat');
      assert.equal(document.querySelector('dialog'), null);
      await click(button('Create channel'));
      assert.equal(document.querySelector('dialog input').value, '');
    } finally { await setup.cleanup(); }
  }, { url: `http://localhost${base}?view=chat`, fetch: async (path, options = {}) => {
    if (path.endsWith('/chat-directory')) return response({ users: [{id:'member',name:'Priya'}], agents: [], role: 'Owner' });
    if (path.endsWith('/chats')) {
      if (options.method === 'POST') {
        const body = JSON.parse(options.body); assert.equal(body.kind,'channel'); assert.deepEqual(body.users,['member']);
        created = true; return response({ id: 'created' });
      }
      return response({ chats: created ? [{ id: 'created', name: 'New team chat', kind:'channel',description:'',users:['owner','member'],agents:[] }] : [] });
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
      await until(update, () => document.querySelector('.notification-preview')?.textContent.includes('Daily report'));
      assert.equal(document.querySelector('#home-agents'), null);
      assert.equal(document.querySelectorAll('.home-standard-panels .home-clock-panel').length, 1);
      assert.equal(document.querySelectorAll('.home-standard-panels .home-notifications').length, 1);
      assert.match(document.querySelector('.notification-preview').textContent, /Daily report/);
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
      assert.match(document.querySelector('.notification-preview').textContent, /Send supplier request/);
      await click(document.querySelector('.notification-row'));
      assert.match(document.querySelector('.inbox-filters [aria-pressed=true]').textContent, /Decisions/);
      assert.match(document.querySelector('#home-attention').textContent, /Send supplier request/);
      await click(button('Close inbox'));
      await click(button('Customize Home'));
      assert.equal(document.querySelector('[data-arrange]').hidden, false);
      assert.equal(document.querySelector('.home-standard-panels .panel-arrange'), null);
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
      await until(update, () => document.querySelector('.notifications-clear'));
      assert.equal(document.querySelector('.home-welcome'), null);
      assert.match(document.querySelector('.notifications-clear').textContent, /all caught up/);
      await click(document.querySelector('.home-location'));
      assert.equal(document.querySelector('.clock-dialog').open, true);
      await click(document.querySelector('.clock-dialog [aria-label="Close location"]'));
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

test('unified Chat keeps its directory across agent, channel, and direct-message navigation', async () => {
  const chats = [{id:'inventory',name:'Inventory',kind:'channel',description:'Stock',users:['owner','member'],agents:[]}, {id:'priya',name:'Priya',kind:'direct',description:'',users:['owner','member'],agents:[]}];
  let reply;
  await reactFixture(async ({dom,render,click,update,change,submit,button}) => {
    const setup=await prepare(dom);
    try {
      await render(setup.WorkspaceApplication);
      await until(update,()=>document.querySelector('.chat-contact'));
      const sidebar=document.querySelector('.chat-directory');
      await click([...sidebar.querySelectorAll('a')].find(a=>a.textContent.includes('Inventory')));
      await until(update,()=>document.querySelector('.message-composer textarea'));
      await change(document.querySelector('.message-composer textarea'),'Inventory draft');
      await click([...sidebar.querySelectorAll('a')].find(a=>a.textContent.includes('Ramesh')));
      await until(update,()=>document.querySelector('.message-composer textarea'));
      assert.equal(document.querySelector('.chat-directory'),sidebar);
      await click([...sidebar.querySelectorAll('a')].find(a=>a.textContent.includes('Priya')));
      await until(update,()=>document.querySelector('.chat-pane-header h2')?.textContent==='Priya');
      assert.equal(document.querySelector('.chat-directory'),sidebar);
      assert.equal(setup.streams.size,0);
      await click([...sidebar.querySelectorAll('a')].find(a=>a.textContent.includes('Inventory')));
      await until(update,()=>document.querySelector('.message-composer textarea')?.value==='Inventory draft');
      await click(button('Reply in thread'));
      await change(document.querySelector('textarea[aria-label="Reply in thread"]'),'Thread reply');
      await submit(document.querySelector('dialog .message-composer'));
      assert.match(document.querySelector('[aria-label="Thread replies"]').textContent,/Thread reply/);
    } finally { await setup.cleanup(); }
  },{url:`http://localhost${base}?view=chat`,fetch:async(path,options={})=>{
    path = new URL(path, 'http://localhost').pathname;
    if(path.endsWith('/chat-directory'))return response({users:[{id:'member',name:'Priya'}],agents:profiles.map(p=>({id:p.id,name:p.name})),role:'Owner'});
    if(path.endsWith('/chats'))return response({chats});
    if(path.endsWith('/messages')){
      if(options.method==='POST'){const body=JSON.parse(options.body);assert.equal(body.thread,'root');reply=body.content;return response({ok:true,message:{...body,author:'owner',kind:'user',state:'sent',at:2,sequence:2,chat:'inventory'}});}
      return response({agents:[],messages:[{id:'root',author:'member',name:'Priya',kind:'user',content:'Check stock',state:'sent',at:1,thread:null},...(reply?[{id:'reply',author:'owner',name:'Owner',kind:'user',content:reply,state:'sent',at:2,thread:'root'}]:[])]});
    }
    return response(dataFor(path));
  }});
});

test('rebrands only the built-in guide while preserving custom agent names', async () => {
  const { displayAgentName } = await import('../src/agents.ts');
  for (const name of ['Ayati', 'Relay', 'Cuckoo']) {
    assert.equal(displayAgentName({ id: 'local-agent', name }), 'Cuckoo');
    assert.equal(displayAgentName({ id: 'custom-agent', name }), name);
  }
});
