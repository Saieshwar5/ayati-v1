import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Telegram (runtime/telegram-channel.md): an admin connects the workspace's
// Telegram once; switching it on for an agent asks the Telegram owner to create
// the agent's bot with one tap; people connect their own Telegram in Profile.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const session = { user: { id: 'owner', email: 'owner@example.test', display_name: 'Ravi Kumar' }, workspaces: [{ id: w, name: 'Mehta Furnishings', role: 'Member' }] };
const response = data => ({ ok: true, json: async () => data });
const failure = (status, error) => ({ ok: false, status, json: async () => ({ error }) });
const agent = { id: 'nova', name: 'Nova', role: 'HR', model: 'm', audience: 'everyone', revision: 3, settings: { avatar: 'spark', paused: false, instructions: '' }, editable: true };
const directory = { role: 'Member', agents: [], default_model: 'fixture', system_prompt: 'Use only assigned capabilities.', providers: [] };
const field = label => [...document.querySelectorAll('label')].find(node => node.textContent.startsWith(label)).querySelector('input,textarea,select');
const toggle = () => [...document.querySelectorAll('label')].find(node => node.textContent === 'Activate Telegram')?.querySelector('input');
const status = change => ({ offered: true, connected: true, state: 'off', username: null, link: null, create_link: null, is_owner: true, can_manage: true, ...change });
const links = () => [...document.querySelectorAll('a')].map(node => [node.textContent, node.getAttribute('href')]);
/** window.open as a browser does it: a blank tab, pointed at the link once it is known. */
function tabs() {
  const opened = [];
  window.open = () => { const tab = { location: { href: '' }, close() { tab.closed = true; } }; opened.push(tab); return tab; };
  return opened;
}

test('a new agent asks for its Telegram bot only when its creator switches it on', async () => {
  for (const [connected, turnOn] of [[false, false], [true, false], [true, true]]) {
    const activations = [];
    await reactFixture(async ({ render, button, click, change }) => {
      const context = await import('../src/workspace-context.ts');
      await context.enterWorkspace();
      const { AgentCreate, newAgent } = await import('../src/react/agents/create/create-flow.tsx');
      await render(AgentCreate, { start: newAgent(directory), directory, telegramOffered: true, close() {}, created: async () => {} });
      await change(field('Name'), 'Nova');
      await click(button('Continue'));
      assert.match(document.body.textContent, /People in this workspace talk to Nova on Telegram/);
      assert.equal(toggle().checked, false, 'off by default');
      assert.equal(toggle().disabled, !connected, 'nothing to switch on before an admin connects Telegram');
      if (!connected) assert.match(document.body.textContent, /A workspace admin needs to connect Telegram/);
      if (turnOn) {
        await click(toggle());
        assert.match(document.body.textContent, /Nova gets its own Telegram bot/);
      }
      await click(button('Continue'));
      await click(button('Customer support'));
      for (let next = 0; next < 2; next++) await click(button('Continue'));
      await click(button('Create agent'));
      assert.equal(activations.length, turnOn ? 1 : 0);
      if (turnOn) assert.match(activations[0], /\/agent-profiles\/[^/]+\/telegram\/activate$/);
    }, { url, fetch: async (path, options = {}) => {
      if (path.endsWith('/session')) return response(session);
      if (path.endsWith('/chat-directory')) return response({ users: [], agents: [], role: 'Member' });
      if (path.endsWith('/control/available')) return response({ connections: [], execution_available: true });
      if (path.endsWith(`/workspaces/${w}/telegram`)) return response({ offered: true, connected, owner: connected ? 'Ravi Kumar' : null, is_owner: true, can_manage: true });
      if (path.endsWith('/telegram/activate')) { activations.push(path); return response(status({ state: 'requested' })); }
      const body = JSON.parse(options.body);
      return response({ ...body, revision: 1 });
    } });
  }
});

test('the owner activates Telegram once on Overview, then creates the bot with one tap', async () => {
  const posts = []; let fail = true; let release;
  await reactFixture(async ({ render, button, click, update }) => {
    const { TelegramFact } = await import('../src/react/agents/telegram/telegram-fact.tsx');
    await render(TelegramFact, { agent });
    assert.match(document.body.textContent, /TelegramOff/);
    await click(button('Activate Telegram'));
    assert.match(document.body.textContent, /A workspace admin needs to connect Telegram in Settings/);
    assert.match(document.body.textContent, /TelegramOff/, 'a failure keeps it off');
    fail = false;
    await update(() => { button('Activate Telegram').click(); button('Activate Telegram').click(); });
    assert.equal(posts.length, 2, 'a second click while saving sends nothing');
    await update(() => release());
    assert.match(document.body.textContent, /Waiting for its bot/);
    assert.deepEqual(links(), [['Create bot in Telegram', 'https://t.me/newbot/CuckooAgentsBot/nova_k7q2m_bot?name=Nova']]);
    assert.ok(button('Switch off'));
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/api/identity/telegram')) return response({ offered: true, linked: true, username: 'ravi' });
    if (!options.body) return response(status());
    posts.push(path);
    if (fail) return failure(409, 'A workspace admin needs to connect Telegram in Settings first.');
    await new Promise(resolve => { release = resolve; });
    return response(status({ state: 'requested', create_link: 'https://t.me/newbot/CuckooAgentsBot/nova_k7q2m_bot?name=Nova' }));
  } });
});

test('people who can talk to the agent open its bot; they connect their Telegram first', async () => {
  const active = status({ state: 'active', username: 'nova_k7q2m_bot', link: 'https://t.me/nova_k7q2m_bot', is_owner: false, can_manage: false });
  for (const linked of [true, false]) {
    await reactFixture(async ({ render, button, click }) => {
      const opened = tabs();
      const { TelegramFact } = await import('../src/react/agents/telegram/telegram-fact.tsx');
      await render(TelegramFact, { agent: { ...agent, editable: false } });
      assert.match(document.body.textContent, /@nova_k7q2m_bot/);
      assert.deepEqual(links(), [['Open in Telegram', 'https://t.me/nova_k7q2m_bot']]);
      const labels = [...document.querySelectorAll('button')].map(node => node.textContent);
      assert.deepEqual(labels, linked ? [] : ['Connect your Telegram'], 'no switches for people who may only talk to it');
      if (!linked) {
        await click(button('Connect your Telegram'));
        assert.equal(opened[0].location.href, 'https://t.me/CuckooAgentsBot?start=cABC');
        assert.match(document.body.textContent, /Tap Start in Telegram/);
      }
    }, { url, fetch: async path => {
      if (path.endsWith('/telegram/link')) return response({ link: 'https://t.me/CuckooAgentsBot?start=cABC', expires_in: 900 });
      if (path.endsWith('/api/identity/telegram')) return response({ offered: true, linked, username: linked ? 'ravi' : null });
      return response(active);
    } });
  }
  await reactFixture(async ({ render }) => {
    const { TelegramFact } = await import('../src/react/agents/telegram/telegram-fact.tsx');
    await render(TelegramFact, { agent });
    assert.doesNotMatch(document.body.textContent, /Telegram/, 'nothing when Telegram is not offered');
  }, { url, fetch: async path => path.endsWith('/api/identity/telegram')
    ? response({ offered: false, linked: false, username: null }) : response(status({ offered: false, connected: false })) });
});

test('a person connects their Telegram from Profile, and can disconnect it', async () => {
  let linked = false; const posts = [];
  await reactFixture(async ({ render, button, click }) => {
    const opened = tabs();
    const { TelegramLink } = await import('../src/react/settings/telegram-link.tsx');
    await render(TelegramLink, {});
    assert.match(document.body.textContent, /Only people who connected can use agents on Telegram/);
    await click(button('Connect Telegram'));
    assert.equal(opened[0].location.href, 'https://t.me/CuckooAgentsBot?start=cABC');
    await click(button('I’ve connected'));
    assert.match(document.body.textContent, /Not connected yet/);
    linked = true;
    await click(button('I’ve connected'));
    assert.match(document.body.textContent, /Connected as @ravi/);
    await click(button('Disconnect'));
    assert.deepEqual(posts, ['/api/identity/telegram/link', '/api/identity/telegram/unlink']);
    assert.ok(button('Connect Telegram'));
  }, { url, fetch: async (path, options = {}) => {
    if (options.method === 'POST') posts.push(path);
    if (path.endsWith('/telegram/link')) return response({ link: 'https://t.me/CuckooAgentsBot?start=cABC', expires_in: 900 });
    if (path.endsWith('/telegram/unlink')) { linked = false; return response({ linked: false, username: null }); }
    return response({ offered: true, linked, username: linked ? 'ravi' : null });
  } });
});

test('an admin connects the workspace Telegram once and becomes its Telegram owner', async () => {
  let owner = null;
  await reactFixture(async ({ render, button, click }) => {
    const opened = tabs();
    const { WorkspaceTelegram } = await import('../src/react/settings/workspace-telegram.tsx');
    await render(WorkspaceTelegram, {});
    await click(button('Connect Telegram'));
    assert.equal(opened[0].location.href, 'https://t.me/CuckooAgentsBot?start=cOWN');
    owner = 'Ravi Kumar';
    await click(button('Check again'));
    assert.match(document.body.textContent, /Connected by Ravi Kumar \(you\)/);
    assert.equal(document.querySelector('button'), null, 'nothing more to do for the owner');
  }, { url, fetch: async path => {
    if (path.endsWith('/telegram/connect')) return response({ link: 'https://t.me/CuckooAgentsBot?start=cOWN', expires_in: 900 });
    return response({ offered: true, connected: Boolean(owner), owner, is_owner: Boolean(owner), can_manage: true });
  } });
});
