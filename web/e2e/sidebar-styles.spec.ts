import { test, expect } from '@playwright/test';
import { createServer, type Server } from 'node:http';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const workspace = '11111111-1111-4111-8111-111111111111';
const root = resolve(import.meta.dirname, '../dist');
const agents = ['spark', 'flower'].map((avatar, index) => ({
  id: `agent-${index}`, name: `Assistant ${index}`, role: 'Review local records',
  scope: 'workspace', revision: 1, model: 'fixture', settings: { avatar }, editable: false,
}));
let server: Server;
let base: string;

test.beforeAll(async () => {
  server = createServer(async (request, response) => {
    const path = new URL(request.url!, 'http://localhost').pathname;
    const asset = path.startsWith('/assets/') || path.startsWith('/brand/');
    const file = resolve(root, asset ? `.${path}` : 'index.html');
    if (!file.startsWith(root + '/')) { response.writeHead(404).end(); return; }
    try {
      const type = file.endsWith('.css') ? 'text/css' : file.endsWith('.js') ? 'text/javascript'
        : file.endsWith('.svg') ? 'image/svg+xml' : 'text/html';
      response.writeHead(200, { 'Content-Type': type }).end(await readFile(file));
    } catch { response.writeHead(404).end(); }
  });
  await new Promise<void>(done => server.listen(0, '127.0.0.1', done));
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('Test server unavailable');
  base = `http://127.0.0.1:${address.port}/w/${workspace}/`;
});
test.afterAll(async () => { await new Promise<void>((done, reject) => server.close(error => error ? reject(error) : done())); });

for (const theme of ['light', 'dark'] as const) {
  test(`sidebar avatar containment and selection states survive navigation in ${theme}`, async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(String(error)));
    await page.emulateMedia({ colorScheme: theme });
    await page.route('**/api/**', async route => {
      const path = new URL(route.request().url()).pathname;
      let data: unknown = {};
      if (path.endsWith('/session')) data = { user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: workspace, name: 'Store', role: 'Owner' }] };
      else if (path.endsWith('/agent-profiles')) data = { agents, role: 'Owner', providers: [], default_model: 'fixture', system_prompt: '' };
      else if (path.endsWith('/chat-directory')) data = { users: [], agents: [], role: 'Owner' };
      else if (path.endsWith('/chats')) data = { chats: ['inventory', 'updates'].map(id => ({ id, name: id, kind: 'channel', description: '', users: ['owner'], agents: [] })) };
      else if (path.endsWith('/messages')) data = { messages: [], agents: [] };
      else if (path.endsWith('/agents')) data = { agents: [], creation_available: false };
      else if (path.endsWith('/unread')) data = { unread: [] };
      else if (path.endsWith('/message-capabilities')) data = { live: false };
      await route.fulfill({ json: data });
    });
    await page.goto(`${base}?view=agents`);
    const nav = page.getByRole('navigation', { name: 'Workspace', exact: true });
    const selected = nav.getByRole('link', { name: 'Agents', exact: true });
    const contain = async () => {
      await expect(page.locator('.agent-profile-avatar')).toHaveCount(2);
      await expect(page.locator('.agent-profile-avatar').first()).toHaveCSS('font-size', '24px');
      const positions = await page.locator('.agent-profile-avatar').evaluateAll(nodes => nodes.map(node => {
        const avatar = node.getBoundingClientRect(), row = node.closest('.ui-row')!.getBoundingClientRect();
        return { contained: avatar.x >= row.x && avatar.y >= row.y && avatar.right <= row.right && avatar.bottom <= row.bottom,
          after: getComputedStyle(node, '::after').content, position: getComputedStyle(node).position };
      }));
      expect(positions.every(p => p.contained && p.after === 'none' && p.position === 'static')).toBe(true);
    };
    await contain();
    const selectedColor = theme === 'light' ? 'rgb(34, 37, 42)' : 'rgb(240, 241, 245)';
    await expect(selected).toHaveCSS('background-color', selectedColor);
    await selected.hover();
    await expect(selected).toHaveCSS('background-color', selectedColor);
    await nav.getByRole('link', { name: 'Chat', exact: true }).hover();
    await expect(selected).toHaveCSS('background-color', selectedColor);
    await selected.click();
    expect(await selected.evaluate(node => node.matches(':focus-visible'))).toBe(false);
    const icon = await selected.locator('svg').boundingBox();
    await page.keyboard.press('Tab'); await page.keyboard.press('Shift+Tab');
    await expect(selected).toHaveCSS('outline-style', 'solid');
    await expect(selected).toHaveCSS('outline-width', '2px');
    await expect(selected).toHaveCSS('outline-offset', '-3px');
    await expect(selected).toHaveCSS('outline-color', theme === 'light' ? 'rgb(255, 255, 255)' : 'rgb(32, 34, 42)');
    expect(await selected.locator('svg').boundingBox()).toEqual(icon);
    await expect(selected).toHaveCSS('background-color', selectedColor);
    await nav.getByRole('link', { name: 'Chat', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Chat', exact: true })).toBeVisible();
    const channel = page.getByRole('complementary', { name: 'Conversations' }).getByRole('link', { name: /inventory/ });
    await channel.hover();
    await expect(channel).toHaveCSS('background-color', theme === 'light' ? 'rgb(247, 248, 249)' : 'rgb(37, 42, 52)');
    await channel.click(); await expect(channel).toHaveAttribute('aria-current', 'page');
    await expect(channel).toHaveCSS('background-color', theme === 'light' ? 'rgb(238, 240, 242)' : 'rgb(48, 53, 64)');
    await channel.hover();
    await expect(channel).toHaveCSS('background-color', theme === 'light' ? 'rgb(238, 240, 242)' : 'rgb(48, 53, 64)');
    await selected.click(); await contain();
    await page.setViewportSize({ width: 320, height: 568 }); await contain();
    await page.reload(); await contain();
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    expect(errors).toEqual([]);
  });
}
