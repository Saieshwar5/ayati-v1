import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

test('React account menu escapes identity text, scopes workspace links, and recovers from logout failure', async () => {
  const first = '11111111-1111-4111-8111-111111111111';
  const second = '22222222-2222-4222-8222-222222222222';
  let failLogout = true;
  await reactFixture(async ({ render, click, navigation, dom }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { WorkspaceShell } = await import('../src/react/workspace-shell.tsx');
    await render(WorkspaceShell, { page: 'home', children: 'Home content' });
    assert.equal(document.querySelectorAll('aside .workspace-profile-trigger').length, 1);
    assert.equal(document.querySelector('#workspace-switcher'), null);
    assert.equal(document.querySelector('aside').lastElementChild.classList.contains('workspace-profile-trigger'), true);
    assert.equal(document.querySelector('img,script'), null);
    const choices = [...document.querySelectorAll('#workspace-profile .workspace-menu-list a')];
    assert.deepEqual(choices.map(anchor => anchor.getAttribute('href')), [`/w/${first}/`, `/w/${second}/`]);
    assert.equal(choices[0].getAttribute('aria-current'), 'true');
    assert.equal(document.querySelector('#workspace-profile').getAttribute('popover'), 'auto');
    assert.equal(document.querySelector('aside').textContent.includes('@example.com'), false);
    assert.equal(document.querySelector('.workspace-nav[aria-current]').textContent, 'Home');
    assert.ok([...document.querySelectorAll('.workspace-nav')].every(link => link.pathname.startsWith(`/w/${first}/`)));
    const logout = document.querySelector('#workspace-profile button');
    await click(logout); assert.equal(logout.disabled, false); assert.deepEqual(navigation, []);
    assert.match(document.querySelector('[role=status]').textContent, /failed/);
    failLogout = false; await click(logout); assert.deepEqual(navigation, ['/?view=account']);
    assert.equal(dom.window.document.querySelectorAll('.workspace-shell').length, 1);
  }, { url: `http://localhost/w/${first}/`, fetch: async (path, options) => {
    if (path.endsWith('/logout')) {
      assert.equal(options.method, 'POST'); assert.equal(options.headers['X-Ayati-Request'], 'identity');
      if (failLogout) throw new Error('offline');
      return { ok: true, json: async () => ({ ok: true }) };
    }
    if (path.endsWith('/context')) return { ok: true, json: async () => ({}) };
    return { ok: true, json: async () => ({ user: { id: 'one', email: '<img src=x>@example.com' }, workspaces: [{ id: first, name: '<script>store</script>', role: 'Owner' }, { id: second, name: 'Studio', role: 'Member' }] }) };
  } });
});
