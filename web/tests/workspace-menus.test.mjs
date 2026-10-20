import { test } from 'node:test';
import assert from 'node:assert/strict';
import { JSDOM } from 'jsdom';

test('menus keep workspace links scoped, escape identity text and recover from logout failure', async () => {
  const previous = { document: globalThis.document, location: globalThis.location, fetch: globalThis.fetch, setInterval: globalThis.setInterval, localStorage: globalThis.localStorage };
  const first = '11111111-1111-4111-8111-111111111111';
  const second = '22222222-2222-4222-8222-222222222222';
  const dom = new JSDOM('<aside><a id="logo"></a><nav></nav></aside>', { url: 'http://localhost' });
  let redirected = ''; let failLogout = true;
  try {
    globalThis.document = dom.window.document;
    globalThis.location = { pathname: `/w/${first}/`, assign: path => { redirected = path; } };
    globalThis.localStorage = { setItem() {} }; globalThis.setInterval = () => 0;
    globalThis.fetch = async (path, options) => {
      if (path.endsWith('/logout')) {
        assert.equal(options.method, 'POST');
        assert.equal(options.headers['X-Ayati-Request'], 'identity');
        if (failLogout) throw new Error('offline');
        return { ok: true, json: async () => ({ ok: true }) };
      }
      return { ok: true, json: async () => ({ user: { id: 'one', email: '<img src=x>@example.com' }, workspaces: [{ id: first, name: '<script>store</script>', role: 'Member' }, { id: second, name: 'Studio', role: 'Member' }] }) };
    };
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { installWorkspaceMenus } = await import('../src/workspace-menus.ts');
    installWorkspaceMenus(document.querySelector('aside'));
    assert.equal(document.querySelectorAll('aside button').length, 1);
    assert.equal(document.querySelector('#workspace-switcher'), null);
    assert.equal(document.querySelector('aside').lastElementChild.classList.contains('workspace-profile-trigger'), true);
    assert.equal(document.querySelector('img,script'), null);
    const choices = [...document.querySelectorAll('#workspace-profile .workspace-menu-list a')];
    assert.deepEqual(choices.map(a => a.getAttribute('href')), [`/w/${first}/`, `/w/${second}/`]);
    assert.equal(choices[0].getAttribute('aria-current'), 'true');
    assert.equal(document.querySelector('#workspace-profile').popover, 'auto');
    assert.equal(document.querySelector('aside').textContent.includes('@example.com'), false);
    const logout = document.querySelector('#workspace-profile button');
    await logout.onclick(); assert.equal(logout.disabled, false); assert.equal(redirected, '');
    assert.match(document.querySelector('[role=status]').textContent, /failed/);
    failLogout = false; await logout.onclick(); assert.equal(redirected, '/?view=account');
  } finally { Object.assign(globalThis, previous); dom.window.close(); }
});
