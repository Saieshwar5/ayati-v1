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
const profiles = [{ id: 'assistant-two', name: 'Ramesh' }];
const response = data => ({ ok: true, status: 200, json: async () => data });
function dataFor(path) {
  if (path.endsWith('/session')) return session;
  if (path.endsWith('/chat-directory')) return { users: [], agents: profiles.map(p => ({ id:p.id, name:p.name })), role: 'Owner' };
  if (path.endsWith('/message-capabilities')) return { live: true };
  if (path.endsWith('/unread')) return { unread: [] };
  if (path.endsWith('/chats')) return { chats: [] };
  if (path.endsWith('/work')) return { work: [] };
  if (path.includes('/attention?')) return attention;
  if (path.endsWith('/home-panels')) return { available: false, panels: [] };
  if (path.endsWith('/people')) return { role: 'Owner', members: [{ id: 'owner', email: 'owner@example.com', role: 'Owner' }], invitations: [] };
  throw new Error(`Unexpected request: ${path}`);
}
let attention = { available: true, items: [], next_before: null };
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
    await act(async () => { await Promise.resolve(); });
    Object.assign(globalThis, original);
  } };
}
async function until(update, condition) {
  for (let i = 0; i < 80 && !condition(); i++) await update();
  assert.ok(condition(), 'Expected UI did not appear: '+document.body.textContent);
}

test('sidebar navigation retains the shell, handles rapid changes and cleans feature timers', async () => {
  await reactFixture(async ({ dom, render, click, update }) => {
    const setup = await prepare(dom);
    try {
      await render(setup.WorkspaceApplication);
      await until(update, () => document.querySelector('.home'));
      const rail = document.querySelector('.workspace-rail');
      assert.equal(rail.querySelector('[title=Apps]'), null);
      for (const [label, selector] of [['Chat', '.workspace-chat'], ['Settings', '.settings-page'], ['Home', '.home']]) {
        await click(rail.querySelector(`[title=${label}]`));
        assert.equal(document.querySelector('.workspace-rail'), rail);
        await until(update, () => document.querySelector(selector));
      }
      // Two clicks before features settle must leave only the last route mounted.
      await update(() => {
        rail.querySelector('[title=Chat]').click();
        rail.querySelector('[title=Settings]').click();
      });
      await until(update, () => document.querySelector('.settings-page'));
      assert.equal(document.querySelector('.workspace-chat'), null);
      assert.equal(setup.timers.size, 1, 'Only the workspace outbox/receipt timer remains');
      await update(() => dom.window.history.back());
      await until(update, () => document.querySelector('.workspace-chat'));
      assert.equal(document.querySelector('.workspace-rail'), rail);
      assert.equal(document.querySelector('[title=Chat]').getAttribute('aria-current'), 'page');
    } finally { await setup.cleanup(); }
  }, { url: `http://localhost${base}?view=home`, fetch: async path => response(dataFor(path)) });
});

test('navigation interception leaves workspace switches, downloads and other destinations native', async () => {
  await reactFixture(async ({ dom }) => {
    const { localDestination, workspacePage } = await import('../src/navigation.ts');
    const current = new URL(dom.window.location.href);
    // Links to removed local screens land on Home instead of a dead view.
    assert.equal(workspacePage(new URL('?agent=assistant-two', current)), 'home');
    assert.equal(workspacePage(new URL('?view=apps', current)), 'home');
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

test('Home shows the clock, shared attention and saved panels without private runtime controls', async () => {
  attention = { available: true, next_before: null, items: [{ kind: 'outcome', id: 'finished', version: '1', title: 'Daily report', status: 'complete', at: 1, agent: 'assistant-two', agent_name: 'Ramesh', chat: 'reports', thread: null, reply: 'reply', conversation_name: 'Reports', archived: false, read: false, reviewed: false }] };
  await reactFixture(async ({ dom, render, update }) => {
    const setup = await prepare(dom);
    try {
      await render(setup.WorkspaceApplication);
      await until(update, () => document.querySelector('.notification-preview')?.textContent.includes('Daily report'));
      assert.equal(document.querySelectorAll('.home-standard-panels .home-clock-panel').length, 1);
      assert.equal(document.querySelectorAll('.home-standard-panels .home-notifications').length, 1);
      assert.match(document.querySelector('.home-location').textContent, /^Device time/);
      for (const legacy of ['#home-history-toggle', '#home-refresh', '#home-guide']) assert.equal(document.querySelector(legacy), null);
    } finally { attention = { available: true, items: [], next_before: null }; await setup.cleanup(); }
  }, { url: `http://localhost${base}?view=home`, fetch: async path => response(dataFor(path)) });
});

test('unified Chat keeps its directory across channel and direct-message navigation', async () => {
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
