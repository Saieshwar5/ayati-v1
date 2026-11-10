import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// WhatsApp on numbers Cuckoo provides (runtime/whatsapp-channel.md): off until the
// creator switches it on; the number shows on the agent; people link their own
// WhatsApp number in Profile by sending a code from it.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const session = { user: { id: 'owner', email: 'owner@example.test', display_name: 'Ravi Kumar' }, workspaces: [{ id: w, name: 'Mehta Furnishings', role: 'Member' }] };
const response = data => ({ ok: true, json: async () => data });
const failure = (status, error) => ({ ok: false, status, json: async () => ({ error }) });
const agent = { id: 'nova', name: 'Nova', role: 'HR', model: 'm', audience: 'everyone', revision: 3, settings: { avatar: 'spark', paused: false, instructions: '' }, editable: true };
const directory = { role: 'Member', agents: [], default_model: 'fixture', system_prompt: 'Use only assigned capabilities.', providers: [] };
const field = label => [...document.querySelectorAll('label')].find(node => node.textContent.startsWith(label)).querySelector('input,textarea,select');
const toggle = () => [...document.querySelectorAll('label')].find(node => node.textContent === 'Activate WhatsApp')?.querySelector('input');

test('a new agent gets a WhatsApp number only when its creator activates it', async () => {
  for (const turnOn of [false, true]) {
    const activations = [];
    await reactFixture(async ({ render, button, click, change }) => {
      const context = await import('../src/workspace-context.ts');
      await context.enterWorkspace();
      const { AgentCreate, newAgent } = await import('../src/react/agents/create/create-flow.tsx');
      await render(AgentCreate, { start: newAgent(directory), directory, whatsappOffered: true, close() {}, created: async () => {} });
      await change(field('Name'), 'Nova');
      await click(button('Continue'));
      assert.match(document.body.textContent, /People in this workspace message Nova on WhatsApp/);
      assert.equal(toggle().checked, false, 'off by default');
      if (turnOn) {
        await click(toggle());
        assert.match(document.body.textContent, /Nova gets its own WhatsApp number when you create it/);
      }
      await click(button('Continue'));
      await click(button('Customer support'));
      for (let next = 0; next < 2; next++) await click(button('Continue'));
      await click(button('Create agent'));
      assert.equal(activations.length, turnOn ? 1 : 0);
      if (turnOn) assert.match(activations[0], /\/agent-profiles\/[^/]+\/whatsapp\/activate$/);
    }, { url, fetch: async (path, options = {}) => {
      if (path.endsWith('/session')) return response(session);
      if (path.endsWith('/chat-directory')) return response({ users: [], agents: [], role: 'Member' });
      if (path.endsWith('/control/available')) return response({ connections: [], execution_available: true });
      if (path.endsWith('/whatsapp/activate')) { activations.push(path); return response({ offered: true, state: 'active', number: '+14155550101', can_manage: true }); }
      const body = JSON.parse(options.body);
      return response({ ...body, revision: 1 });
    } });
  }
});

test('the agent has no WhatsApp choice when this Cuckoo offers none', async () => {
  await reactFixture(async ({ render, button, click, change }) => {
    const context = await import('../src/workspace-context.ts');
    await context.enterWorkspace();
    const { AgentCreate, newAgent } = await import('../src/react/agents/create/create-flow.tsx');
    await render(AgentCreate, { start: newAgent(directory), directory, close() {}, created: async () => {} });
    await change(field('Name'), 'Nova');
    await click(button('Continue'));
    assert.equal(toggle(), undefined);
  }, { url, fetch: async path => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/control/available')) return response({ connections: [], execution_available: true });
    return response({ users: [], agents: [], role: 'Member' });
  } });
});

test('the owner activates WhatsApp once on Overview and sees the number; a failure keeps it off', async () => {
  const posts = []; let fail = true; let release;
  await reactFixture(async ({ render, button, click, update }) => {
    const { WhatsappFact } = await import('../src/react/agents/whatsapp/whatsapp-fact.tsx');
    await render(WhatsappFact, { agent });
    assert.match(document.body.textContent, /WhatsAppOff/);
    await click(button('Activate WhatsApp'));
    assert.match(document.body.textContent, /No WhatsApp number is free right now/);
    assert.match(document.body.textContent, /WhatsAppOff/);
    fail = false;
    await update(() => { button('Activate WhatsApp').click(); button('Activate WhatsApp').click(); });
    assert.equal(posts.length, 2, 'a second click while saving sends nothing');
    await update(() => release());
    assert.match(document.body.textContent, /\+1 415 555 0101/);
    assert.ok(button('Copy'));
    assert.ok(button('Switch off'));
    assert.match(document.body.textContent, /People who link their WhatsApp number in their profile can message this number/);
  }, { url, fetch: async (path, options = {}) => {
    if (!options.body) return response({ offered: true, state: 'off', number: null, can_manage: true });
    posts.push(path);
    if (fail) return failure(409, 'No WhatsApp number is free right now. Try again later.');
    await new Promise(resolve => { release = resolve; });
    return response({ offered: true, state: 'active', number: '+14155550101', can_manage: true });
  } });
});

test('people who can talk to the agent see its number without changing it', async () => {
  await reactFixture(async ({ render }) => {
    const { WhatsappFact } = await import('../src/react/agents/whatsapp/whatsapp-fact.tsx');
    await render(WhatsappFact, { agent: { ...agent, editable: false } });
    assert.match(document.body.textContent, /\+91 98000 00001 · Paused by an admin/);
    const labels = [...document.querySelectorAll('button')].map(node => node.textContent);
    assert.deepEqual(labels, ['Copy']);
  }, { url, fetch: async () => response({ offered: true, state: 'paused', number: '+919800000001', can_manage: false }) });
  await reactFixture(async ({ render }) => {
    const { WhatsappFact } = await import('../src/react/agents/whatsapp/whatsapp-fact.tsx');
    await render(WhatsappFact, { agent });
    assert.doesNotMatch(document.body.textContent, /WhatsApp/, 'nothing when WhatsApp is not offered');
  }, { url, fetch: async () => response({ offered: false, state: 'off', number: null, can_manage: true }) });
});

test('a person links their WhatsApp number by sending a code, and can unlink it', async () => {
  let linked = null; const posts = [];
  await reactFixture(async ({ render, button, click }) => {
    const { WhatsappLink } = await import('../src/react/settings/whatsapp-link.tsx');
    await render(WhatsappLink, {});
    assert.match(document.body.textContent, /People without a linked number can’t use WhatsApp with Cuckoo/);
    await click(button('Link WhatsApp'));
    const code = document.querySelector('[aria-label="Link code"]');
    assert.equal(code.querySelector('code').textContent, 'LINK-ABC234');
    assert.match(code.textContent, /Send this code from your WhatsApp to \+1 415 555 0101\. It works for 15 minutes/);
    assert.equal(code.querySelector('a').getAttribute('href'), 'https://wa.me/14155550101?text=LINK-ABC234');
    await click(button('I’ve sent it'));
    assert.match(document.body.textContent, /Not linked yet/);
    linked = '+919800000001';
    await click(button('I’ve sent it'));
    assert.match(document.body.textContent, /Linked: \+91 98000 00001/);
    assert.equal(document.querySelector('[aria-label="Link code"]'), null);
    await click(button('Unlink'));
    assert.deepEqual(posts, ['/api/identity/mobile/link', '/api/identity/mobile/unlink']);
    assert.ok(button('Link WhatsApp'));
  }, { url, fetch: async (path, options = {}) => {
    if (options.method === 'POST') posts.push(path);
    if (path.endsWith('/mobile/link')) return response({ code: 'LINK-ABC234', expires_in: 900, send_to: '+14155550101', link: 'https://wa.me/14155550101?text=LINK-ABC234' });
    if (path.endsWith('/mobile/unlink')) { linked = null; return response({ number: null }); }
    return response({ number: linked, offered: true });
  } });
});
