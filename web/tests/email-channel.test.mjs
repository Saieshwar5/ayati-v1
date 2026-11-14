import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Agent email (runtime/email-channel.md, designs/agent-email): off until the
// creator switches it on; the address shows on Overview with Copy for everyone
// who can talk to the agent, Switch on/off for its owner and admins.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const session = { user: { id: 'owner', email: 'owner@example.test', display_name: 'Ravi Kumar' }, workspaces: [{ id: w, name: 'Mehta Furnishings', role: 'Member' }] };
const response = data => ({ ok: true, json: async () => data });
const failure = (status, error) => ({ ok: false, status, json: async () => ({ error }) });
const agent = { id: 'iris', name: 'Iris', role: 'HR', model: 'm', audience: 'everyone', revision: 3, settings: { avatar: 'spark', paused: false, instructions: '' }, editable: true };
const directory = { role: 'Member', agents: [], default_model: 'fixture', system_prompt: 'Use only assigned capabilities.', providers: [] };
const field = label => [...document.querySelectorAll('label')].find(node => node.textContent.startsWith(label)).querySelector('input,textarea,select');
const toggle = () => [...document.querySelectorAll('label')].find(node => node.textContent === 'Activate email')?.querySelector('input');
const ADDRESS = 'iris.mehta-furnishings@mail.cuckoo.onl';
const active = { offered: true, state: 'active', address: ADDRESS, can_manage: true };

test('a new agent gets an address only when its creator switches email on, with a preview', async () => {
  for (const turnOn of [false, true]) {
    const activations = [];
    await reactFixture(async ({ render, button, click, change }) => {
      const context = await import('../src/workspace-context.ts');
      await context.enterWorkspace();
      const { AgentCreate, newAgent } = await import('../src/react/agents/create/create-flow.tsx');
      await render(AgentCreate, { start: newAgent(directory), directory, emailDomain: 'mail.cuckoo.onl', close() {}, created: async () => {} });
      await change(field('Name'), 'Iris');
      await click(button('Continue'));
      assert.match(document.body.textContent, /People in this workspace email Iris and get its answers in the same mail thread/);
      assert.equal(toggle().checked, false, 'off by default');
      if (turnOn) {
        await click(toggle());
        assert.match(document.body.textContent, /Iris gets its own address when you create it, like iris\.mehta-furnishings@mail\.cuckoo\.onl/);
      }
      await click(button('Continue'));
      await click(button('Customer support'));
      for (let next = 0; next < 2; next++) await click(button('Continue'));
      await click(button('Create agent'));
      assert.equal(activations.length, turnOn ? 1 : 0);
      if (turnOn) assert.match(activations[0], /\/agent-profiles\/[^/]+\/email\/activate$/);
    }, { url, fetch: async (path, options = {}) => {
      if (path.endsWith('/session')) return response(session);
      if (path.endsWith('/chat-directory')) return response({ users: [], agents: [], role: 'Member' });
      if (path.endsWith('/control/available')) return response({ connections: [], execution_available: true });
      if (path.endsWith('/email/activate')) { activations.push(path); return response(active); }
      const body = JSON.parse(options.body);
      return response({ ...body, revision: 1 });
    } });
  }
});

test('there is no email choice when this Cuckoo offers none', async () => {
  await reactFixture(async ({ render, button, click, change }) => {
    const context = await import('../src/workspace-context.ts');
    await context.enterWorkspace();
    const { AgentCreate, newAgent } = await import('../src/react/agents/create/create-flow.tsx');
    await render(AgentCreate, { start: newAgent(directory), directory, close() {}, created: async () => {} });
    await change(field('Name'), 'Iris');
    await click(button('Continue'));
    assert.equal(toggle(), undefined);
  }, { url, fetch: async path => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/control/available')) return response({ connections: [], execution_available: true });
    return response({ users: [], agents: [], role: 'Member' });
  } });
});

test('the owner switches email on once on Overview; a failure keeps it off', async () => {
  const posts = []; let fail = true; let release;
  await reactFixture(async ({ render, button, click, update }) => {
    const context = await import('../src/workspace-context.ts');
    await context.enterWorkspace();
    const { EmailFact } = await import('../src/react/agents/email/email-fact.tsx');
    await render(EmailFact, { agent });
    assert.match(document.body.textContent, /EmailOff/);
    assert.match(document.body.textContent, /Gives Iris its own email address/);
    await click(button('Switch on'));
    assert.match(document.body.textContent, /Email is unavailable right now/);
    assert.match(document.body.textContent, /EmailOff/);
    fail = false;
    await update(() => { button('Switch on').click(); button('Switch on').click(); });
    assert.equal(posts.length, 2, 'a second click while saving sends nothing');
    assert.match(document.body.textContent, /Saving…/);
    await update(() => release());
    assert.match(document.body.textContent, new RegExp(ADDRESS.replace(/\./g, '\\.')));
    assert.ok(button('Copy'));
    assert.ok(button('Switch off'));
    assert.match(document.body.textContent, /People in Mehta Furnishings email this address from the address they sign in with/);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (!options.body) return response({ offered: true, state: 'off', address: null, can_manage: true });
    posts.push(path);
    if (fail) return failure(503, 'Email is unavailable right now. Try again later.');
    await new Promise(resolve => { release = resolve; });
    return response(active);
  } });
});

test('people who can talk to the agent copy its address; a switched-off address stays shown; none when not offered', async () => {
  await reactFixture(async ({ render, button, click }) => {
    const { EmailFact } = await import('../src/react/agents/email/email-fact.tsx');
    await render(EmailFact, { agent: { ...agent, editable: false } });
    const labels = [...document.querySelectorAll('button')].map(node => node.textContent);
    assert.deepEqual(labels, ['Copy']);
    const copied = [];
    const clipboard = { writeText: async text => { copied.push(text); } };
    for (const nav of [window.navigator, globalThis.navigator]) Object.defineProperty(nav, 'clipboard', { configurable: true, value: clipboard });
    await click(button('Copy'));
    assert.ok(button('Copied'));
    assert.deepEqual(copied, [ADDRESS]);
  }, { url, fetch: async () => response({ ...active, can_manage: false }) });
  await reactFixture(async ({ render, button }) => {
    const { EmailFact } = await import('../src/react/agents/email/email-fact.tsx');
    await render(EmailFact, { agent });
    assert.match(document.body.textContent, new RegExp(`Off · ${ADDRESS.replace(/\./g, '\\.')}`));
    assert.match(document.body.textContent, /Switching on brings the same address back/);
    assert.ok(button('Switch on'));
  }, { url, fetch: async () => response({ offered: true, state: 'off', address: ADDRESS, can_manage: true }) });
  await reactFixture(async ({ render }) => {
    const { EmailFact } = await import('../src/react/agents/email/email-fact.tsx');
    await render(EmailFact, { agent });
    assert.doesNotMatch(document.body.textContent, /Email/, 'nothing when email is not offered');
  }, { url, fetch: async () => response({ offered: false, state: 'off', address: null, can_manage: true }) });
});

test('the address preview writes names the way the server does', async () => {
  const { addressPart, addressPreview } = await import('../src/react/agents/email/model.ts');
  assert.equal(addressPart('  HR Helper (India) ', 'agent'), 'hr-helper-india');
  assert.equal(addressPart('Ünïcode Støre', 'team'), 'ncode-stre');
  assert.equal(addressPart('日本', 'team'), 'team');
  assert.equal(addressPart('a very long agent name that keeps going', 'agent'), 'a-very-long-agent-name-t');
  assert.equal(addressPreview('Iris', 'Mehta Furnishings', 'mail.cuckoo.onl'), ADDRESS);
});
