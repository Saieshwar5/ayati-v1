import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const token = 'abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG';
const workspace = '11111111-1111-4111-8111-111111111111';
const ok = data => ({ ok: true, status: 200, json: async () => data });
const pending = { status: 'pending', workspace: 'Mehta Furnishings', inviter: 'Arjun Mehta', role: 'User', expires: Date.now() / 1000 + 86400 * 6, email_hint: 'r***@gmail.com' };
const settle = update => update(async () => { await new Promise(resolve => setTimeout(resolve, 0)); });

test('signed out: the invitation is shown and sign-in returns to it', async () => {
  await reactFixture(async ({ render, update }) => {
    const { InvitePage } = await import('../src/react/access/invite-page.tsx'); await render(InvitePage, { token }); await settle(update);
    assert.match(document.body.textContent, /Arjun Mehta invited you to Mehta Furnishings/);
    assert.match(document.body.textContent, /expires in 6 days/);
    assert.match(document.body.textContent, /r\*\*\*@gmail\.com/);
    const links = [...document.querySelectorAll('a.access-button')].map(a => a.getAttribute('href'));
    assert.deepEqual(links, [`/auth/login?return_to=${encodeURIComponent(`/invite/${token}`)}&method=google`, `/auth/login?return_to=${encodeURIComponent(`/invite/${token}`)}`]);
  }, { url: `http://localhost/invite/${token}`, fetch: async path => path.startsWith('/api/identity/invite-links/') ? ok(pending) : { ok: false, status: 401, json: async () => ({ user: null }) } });
});

test('signed in with another email: no accept, only switching account', async () => {
  await reactFixture(async ({ render, update, button }) => {
    const { InvitePage } = await import('../src/react/access/invite-page.tsx'); await render(InvitePage, { token }); await settle(update);
    assert.match(document.body.textContent, /This invitation is for r\*\*\*@gmail\.com/);
    assert.equal(button('Accept and join'), undefined); assert.ok(button('Use another account'));
  }, { url: `http://localhost/invite/${token}`, fetch: async path => path === '/api/identity/session'
    ? ok({ user: { id: 'u', email: 'someone@else.com', display_name: 'Someone' } }) : ok({ ...pending, matches: false }) });
});

test('the invited person accepts, sets up a profile and opens the workspace', async () => {
  const writes = [];
  await reactFixture(async ({ render, update, button, click, change, submit, navigation }) => {
    const { InvitePage } = await import('../src/react/access/invite-page.tsx'); await render(InvitePage, { token }); await settle(update);
    assert.deepEqual(writes, [], 'opening the link never accepts it');
    await click(button('Accept and join'));
    assert.deepEqual(writes[0], { path: `/api/identity/invite-links/${token}/accept`, body: {} });
    assert.match(document.body.textContent, /Set up your profile/);
    const form = document.querySelector('form');
    assert.equal(form.elements[1].value, 'ravi', 'the name starts from the email');
    await change(form.elements[1], 'Ravi Kumar'); await change(form.elements[2], 'Warehouse lead'); await submit(form);
    assert.deepEqual(writes[1], { path: `/api/identity/workspaces/${workspace}/my-profile`, body: { display_name: 'Ravi Kumar', title: 'Warehouse lead', about: '' } });
    assert.deepEqual(navigation, [`/w/${workspace}/`]);
  }, { url: `http://localhost/invite/${token}`, fetch: async (path, options = {}) => {
    if (options.method === 'POST') { writes.push({ path, body: JSON.parse(options.body) }); return ok(path.endsWith('/accept') ? { id: workspace } : {}); }
    return path === '/api/identity/session' ? ok({ user: { id: 'u', email: 'ravi@gmail.com', display_name: null } }) : ok({ ...pending, matches: true });
  } });
});

test('expired or withdrawn links explain what to do instead of failing silently', async () => {
  for (const [status, text] of [['expired', /has expired/], ['invalid', /not valid or was withdrawn/], ['withdrawn', /not valid or was withdrawn/]]) {
    await reactFixture(async ({ render, update }) => {
      const { InvitePage } = await import('../src/react/access/invite-page.tsx'); await render(InvitePage, { token }); await settle(update);
      assert.match(document.body.textContent, text); assert.match(document.body.textContent, /send a new invitation/);
    }, { url: `http://localhost/invite/${token}`, fetch: async path => path === '/api/identity/session' ? ok({ user: null }) : ok({ status }) });
  }
});
