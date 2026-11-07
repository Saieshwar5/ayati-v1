import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

const first = '11111111-1111-4111-8111-111111111111';
const second = '22222222-2222-4222-8222-222222222222';
const photo = '0b3c9d2e-6f1a-4c55-9e1d-7a2b3c4d5e6f';
const ok = data => ({ ok: true, status: 200, json: async () => data });
// What a screen reader announces: text without the decorative, aria-hidden parts.
const named = node => { const copy = node.cloneNode(true); copy.querySelectorAll('[aria-hidden=true]').forEach(part => part.remove()); return copy.textContent.trim(); };
const menu = () => document.querySelector('#workspace-profile');
const trigger = () => document.querySelector('aside button[aria-label="Your account"]');
const items = () => [...menu().querySelectorAll('a, button')].map(named);

/** The rail with its account menu; `respond` answers the requests the menu makes. */
async function shell(run, { simulation = false, user = { id: 'one', email: 'priya@example.com', display_name: 'Priya Shah' }, respond = () => undefined } = {}) {
  const requests = [];
  const hidden = [];
  await reactFixture(async context => {
    // jsdom has no popover API; record what the menu asks the browser to do.
    context.dom.window.HTMLElement.prototype.hidePopover = function () { hidden.push(this.id); };
    const workspace = await import('../src/workspace-context.ts'); await workspace.enterWorkspace();
    const { WorkspaceShell } = await import('../src/react/workspace-shell.tsx');
    await context.render(WorkspaceShell, { page: 'home', children: 'Home content' });
    const toggle = async state => context.update(() => {
      const event = new context.dom.window.Event('toggle');
      Object.defineProperty(event, 'newState', { value: state });
      menu().dispatchEvent(event);
    });
    await run({ ...context, requests, hidden, toggle });
  }, { url: `http://localhost/w/${first}/`, fetch: async (path, options = {}) => {
    requests.push(path);
    const answer = await respond(path, options);
    if (answer) return answer;
    if (path.endsWith('/chat-directory')) return ok({ role: 'Admin', agents: [], users: [{ id: 'one', name: 'Priya Shah', photo }, { id: 'two', name: 'Ravi' }] });
    return ok({ simulation, user, workspaces: [{ id: first, name: 'Lotus Interiors', role: 'Admin' }, { id: second, name: 'Kapoor Home Studio', role: 'Member' }] });
  } });
}

test('The account menu holds only you, your status, your workspaces, Settings and Sign out (D245)', async () => {
  await shell(async () => {
    assert.equal(document.querySelectorAll('aside button[aria-label="Your account"]').length, 1);
    assert.equal(document.querySelector('aside').lastElementChild.getAttribute('aria-label'), 'Your account');
    assert.equal(menu().getAttribute('popover'), 'auto');
    assert.equal(trigger().getAttribute('popovertarget'), 'workspace-profile');
    assert.equal(named(menu().querySelector('.account-menu-identity')), 'Priya Shah priya@example.com');
    assert.equal(menu().querySelector('h2').textContent, 'Workspaces');
    assert.deepEqual(items(), ['Set a status Available', 'Lotus Interiors Admin', 'Kapoor Home Studio Member', 'All workspaces…', 'Settings', 'Sign out']);
    assert.deepEqual([...menu().querySelectorAll('a')].map(link => link.getAttribute('href')),
      [`/w/${first}/`, `/w/${second}/`, '/?view=account&choose=1', `/w/${first}/?view=settings&section=profile`]);
    // The current workspace is ticked; the others are plain choices.
    assert.deepEqual([...menu().querySelectorAll('.account-menu-workspace')].map(link => [link.getAttribute('aria-current'), link.querySelector('svg') !== null]),
      [['true', true], [null, false]]);
    assert.equal(document.querySelector('aside').textContent.includes('@example.com'), false);
  });
});

test('Your photo shows on the rail and in the menu, and is read again when the menu opens', async () => {
  await shell(async ({ requests, toggle }) => {
    const pictures = () => [trigger(), menu().querySelector('.account-menu-identity')].map(part => part.querySelector('img')?.getAttribute('src'));
    assert.deepEqual(pictures(), Array(2).fill(`/api/identity/workspaces/${first}/members/one/photo/${photo}`));
    const reads = () => requests.filter(path => path.endsWith('/chat-directory')).length;
    const before = reads();
    await toggle('open');
    assert.equal(trigger().getAttribute('aria-expanded'), 'true');
    assert.equal(reads(), before + 1);
    await toggle('closed');
    assert.equal(trigger().getAttribute('aria-expanded'), 'false');
    assert.equal(reads(), before + 1, 'closing reads nothing');
  });
});

test('Without a photo the menu shows initials and escapes identity text', async () => {
  await shell(async () => {
    assert.equal(document.querySelector('img, script') === null, true);
    assert.equal(trigger().textContent.length > 0, true);
    assert.match(menu().textContent, /<script>store<\/script>@example\.com/);
  }, { user: { id: 'one', email: '<script>store</script>@example.com' }, respond: path => path.endsWith('/chat-directory') && ok({ users: [] }) });
});

test('Escape closes the menu and returns focus to your photo', async () => {
  await shell(async ({ update, dom, hidden }) => {
    const settings = [...menu().querySelectorAll('a')].find(link => named(link) === 'Settings');
    await update(() => settings.focus());
    assert.equal(document.activeElement === settings, true);
    await update(() => settings.dispatchEvent(new dom.window.KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })));
    assert.deepEqual(hidden, ['workspace-profile']);
    assert.equal(document.activeElement === trigger(), true);
  });
});

test('Simulations keep the switch-employee entry', async () => {
  await shell(async () => {
    assert.deepEqual(items(), ['Set a status Available', 'Lotus Interiors Admin', 'Kapoor Home Studio Member', 'All workspaces…', 'Simulation · switch employee', 'Settings', 'Sign out']);
    assert.equal([...menu().querySelectorAll('a')].find(link => named(link) === 'Simulation · switch employee').getAttribute('href'), '/index.html?view=simulation');
  }, { simulation: true });
});

test('Sign out sends one request, reports a failure, and returns to sign-in', async () => {
  let fail = true, release;
  const logouts = [];
  await shell(async ({ click, update, navigation }) => {
    const signOut = () => [...menu().querySelectorAll('button')].find(node => named(node) === 'Sign out');
    await click(signOut());
    assert.equal(signOut().disabled, false); assert.deepEqual(navigation, []);
    assert.match(menu().querySelector('[role=status]').textContent, /Sign-out failed/);
    fail = false;
    await update(() => { signOut().click(); signOut().click(); });
    assert.equal(signOut().disabled, true);
    await update(() => release());
    assert.equal(logouts.length, 2, 'the failed attempt and one more');
    assert.deepEqual(navigation, ['/?view=account']);
  }, { respond: (path, options) => {
    if (!path.endsWith('/logout')) return;
    assert.equal(options.method, 'POST'); assert.equal(options.headers['X-Ayati-Request'], 'identity');
    logouts.push(path);
    if (fail) throw new Error('offline');
    return new Promise(resolve => { release = () => resolve(ok({ ok: true })); });
  } });
});
