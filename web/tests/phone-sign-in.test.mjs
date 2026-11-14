import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Profile › Your phone (runtime/device-sessions.md): a one-time QR code signs the
// Cuckoo app in, and every signed-in phone can be signed out from the web.
const response = data => ({ ok: true, json: async () => data });
const failure = (status, error) => ({ ok: false, status, json: async () => ({ error }) });
const start = 1_800_000_000; // server seconds
const phone = (id, name, created) => ({ id, name, platform: id === 'pixel' ? 'android' : 'ios', created, last_used: created, expires: created + 90 * 86400, current: false });
const code = { code: 'one-time-code', expires: start + 600, expires_in: 600, link: 'cuckoo://sign-in?server=http%3A%2F%2Flocalhost&code=one-time-code', qr: 'data:image/svg+xml;base64,PHN2Zy8+' };

/** Runs the page with a controllable clock and the countdown's interval in hand. */
async function withClock(run) {
  const real = { now: Date.now, clear: globalThis.clearInterval };
  const clock = { ms: start * 1000, timers: new Map(), next: 1 };
  Date.now = () => clock.ms;
  try { await run(clock); } finally { Date.now = real.now; globalThis.clearInterval = real.clear; }
}
function captureTimers(clock) {
  globalThis.setInterval = fn => { const id = clock.next++; clock.timers.set(id, fn); return id; };
  globalThis.clearInterval = id => clock.timers.delete(id);
}
const tick = (clock, update, seconds = 1) => update(() => {
  for (let i = 0; i < seconds; i++) { clock.ms += 1000; for (const fn of clock.timers.values()) fn(); }
});

test('a code shows as a QR with a live countdown, expires, and can be hidden or replaced', async () => {
  await withClock(async clock => {
    const posts = [];
    await reactFixture(async ({ render, button, click, update }) => {
      const { PhoneSignIn } = await import('../src/react/settings/phone-sign-in.tsx');
      await render(PhoneSignIn, {});
      assert.match(document.body.textContent, /No phones are signed in/);
      captureTimers(clock);
      await click(button('Sign in on your phone'));
      const qr = document.querySelector('img');
      assert.equal(qr.getAttribute('alt'), 'Sign-in code for your phone');
      assert.equal(qr.getAttribute('src'), code.qr);
      assert.match(document.body.textContent, /Works once, for 10 minutes/);
      assert.equal(document.querySelector('[role=timer]').textContent, 'Expires in 10:00. Don’t share it: whoever scans it signs in as you.');
      await tick(clock, update, 61);
      assert.match(document.querySelector('[role=timer]').textContent, /^Expires in 8:59\./);
      await tick(clock, update, 539);
      assert.equal(document.querySelector('img'), null, 'an expired code is not shown');
      assert.match(document.body.textContent, /This code expired/);
      assert.equal(clock.timers.size, 0, 'the countdown stops once the code expired');
      await click(button('New code'));
      assert.ok(document.querySelector('img'));
      await click(button('Hide'));
      assert.equal(document.querySelector('img'), null);
      assert.equal(clock.timers.size, 0);
      assert.deepEqual(posts, ['/api/identity/devices/link-code', '/api/identity/devices/link-code']);
    }, { fetch: async (path, options = {}) => {
      if (options.method === 'POST') { posts.push(path); return response({ ...code, expires: Math.floor(clock.ms / 1000) + 600 }); }
      return response({ devices: [] });
    } });
  });
});

test('the page notices the scan and the new phone appears in the list', async () => {
  await withClock(async clock => {
    let devices = [phone('old', 'Old iPhone', start - 86400)];
    await reactFixture(async ({ render, button, click, update }) => {
      const { PhoneSignIn } = await import('../src/react/settings/phone-sign-in.tsx');
      await render(PhoneSignIn, {});
      assert.ok(button('Sign out Old iPhone'));
      captureTimers(clock);
      await click(button('Sign in on your phone'));
      devices = [phone('pixel', 'Asha’s Pixel', start + 30), ...devices];
      await tick(clock, update, 4);
      assert.equal(document.querySelector('img'), null);
      assert.match(document.querySelector('.ui-status').textContent, /Signed in on Asha’s Pixel\./);
      assert.match(document.body.textContent, /Asha’s Pixel.*Android · Signed in/);
      assert.equal(clock.timers.size, 0, 'no more polling after the scan');
      assert.ok(button('Sign in on your phone'));
    }, { fetch: async (path, options = {}) => {
      if (options.method === 'POST') return response(code);
      return response({ devices });
    } });
  });
});

test('signing a phone out shows pending, keeps the row on failure and sends once', async () => {
  const posts = []; let answer;
  await reactFixture(async ({ render, button, click, update }) => {
    const { PhoneSignIn } = await import('../src/react/settings/phone-sign-in.tsx');
    await render(PhoneSignIn, {});
    const row = button('Sign out Ravi’s iPhone');
    answer = Promise.withResolvers();
    await click(row);
    assert.equal(row.textContent, 'Signing out…');
    assert.equal(row.disabled, true);
    await click(row);
    answer.resolve(failure(503, 'Workspace database unavailable. Retry shortly.'));
    await update();
    assert.equal(document.querySelector('[role=alert]').textContent, 'Workspace database unavailable. Retry shortly.');
    assert.ok(button('Sign out Ravi’s iPhone'), 'the phone is still listed after a failure');
    answer = Promise.withResolvers();
    await click(button('Sign out Ravi’s iPhone'));
    answer.resolve(response({ revoked: true }));
    await update();
    assert.equal(button('Sign out Ravi’s iPhone'), undefined);
    assert.match(document.querySelector('.ui-status').textContent, /Signed out Ravi’s iPhone\./);
    assert.ok(button('Sign out Old iPhone'));
    assert.deepEqual(posts, ['/api/identity/devices/ravi/revoke', '/api/identity/devices/ravi/revoke']);
  }, { fetch: async (path, options = {}) => {
    if (options.method === 'POST') { posts.push(path); return answer.promise; }
    return response({ devices: [phone('ravi', 'Ravi’s iPhone', start), phone('old', 'Old iPhone', start - 86400)] });
  } });
});

test('a refused code and a failed device list say so', async () => {
  await reactFixture(async ({ render, button, click }) => {
    const { PhoneSignIn } = await import('../src/react/settings/phone-sign-in.tsx');
    await render(PhoneSignIn, {});
    assert.equal(document.querySelector('[role=alert]').textContent, 'Authentication service unavailable. Try again shortly.');
    await click(button('Sign in on your phone'));
    assert.equal(document.querySelector('.ui-status').textContent, 'You made many sign-in codes this hour. Try again later.');
    assert.equal(document.querySelector('img'), null);
  }, { fetch: async (path, options = {}) => options.method === 'POST'
    ? failure(429, 'You made many sign-in codes this hour. Try again later.')
    : failure(503, 'Authentication service unavailable. Try again shortly.') });
});
