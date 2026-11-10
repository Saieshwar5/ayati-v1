import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Email on addresses at Cuckoo's mail domain (runtime/email-channel.md): off until
// the agent's owner or an admin switches it on; the address shows on the agent.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const response = data => ({ ok: true, json: async () => data });
const failure = (status, error) => ({ ok: false, status, json: async () => ({ error }) });
const agent = { id: 'nova', name: 'Nova', role: 'HR', model: 'm', audience: 'everyone', revision: 3, settings: { avatar: 'spark', paused: false, instructions: '' }, editable: true };
const address = 'nova-k7q2m@agents.cuckoo.test';

test('the owner activates email once on Overview and sees the address; a failure keeps it off', async () => {
  const posts = []; let fail = true; let release;
  await reactFixture(async ({ render, button, click, update }) => {
    const { EmailFact } = await import('../src/react/agents/email/email-fact.tsx');
    await render(EmailFact, { agent });
    assert.match(document.body.textContent, /EmailOff/);
    await click(button('Activate email'));
    assert.match(document.body.textContent, /Email is unavailable right now/);
    assert.match(document.body.textContent, /EmailOff/);
    fail = false;
    await update(() => { button('Activate email').click(); button('Activate email').click(); });
    assert.equal(posts.length, 2, 'a second click while saving sends nothing');
    assert.ok(posts.every(path => path.endsWith('/agent-profiles/nova/email/activate')));
    await update(() => release());
    assert.match(document.body.textContent, new RegExp(address));
    assert.ok(button('Copy'));
    assert.ok(button('Switch off'));
    assert.match(document.body.textContent, /write to this address from their sign-in email/);
  }, { url, fetch: async (path, options = {}) => {
    if (!options.body) return response({ offered: true, state: 'off', address: null, can_manage: true });
    posts.push(path);
    if (fail) return failure(503, 'Email is unavailable right now. Try again later.');
    await new Promise(resolve => { release = resolve; });
    return response({ offered: true, state: 'active', address, can_manage: true });
  } });
});

test('switching email off shows it as off again', async () => {
  await reactFixture(async ({ render, button, click }) => {
    const { EmailFact } = await import('../src/react/agents/email/email-fact.tsx');
    await render(EmailFact, { agent });
    await click(button('Switch off'));
    assert.match(document.body.textContent, /EmailOff/);
    assert.ok(button('Activate email'));
  }, { url, fetch: async (path, options = {}) => options.body
    ? response({ offered: true, state: 'off', address: null, can_manage: true })
    : response({ offered: true, state: 'active', address, can_manage: true }) });
});

test('people who can talk to the agent see its address without changing it', async () => {
  await reactFixture(async ({ render }) => {
    const { EmailFact } = await import('../src/react/agents/email/email-fact.tsx');
    await render(EmailFact, { agent: { ...agent, editable: false } });
    assert.match(document.body.textContent, new RegExp(`${address} · Paused`));
    const labels = [...document.querySelectorAll('button')].map(node => node.textContent);
    assert.deepEqual(labels, ['Copy']);
  }, { url, fetch: async () => response({ offered: true, state: 'paused', address, can_manage: false }) });
  await reactFixture(async ({ render }) => {
    const { EmailFact } = await import('../src/react/agents/email/email-fact.tsx');
    await render(EmailFact, { agent });
    assert.doesNotMatch(document.body.textContent, /Email/, 'nothing when email is not offered');
  }, { url, fetch: async () => response({ offered: false, state: 'off', address: null, can_manage: true }) });
});
