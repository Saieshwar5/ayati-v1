import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { cancelRequest } from '../fixtures/chat_actions.mjs';

export async function calibrationBrowser(t, app) {
  const { chromium } = createRequire(new URL('../../web/package.json', import.meta.url))('@playwright/test');
  const browser = await chromium.launch();
  t.after(() => browser.close());
  const context = await browser.newContext({ viewport: { width: 1200, height: 800 } });
  const [name, ...value] = app.cookie.split('=');
  await context.addCookies([{ name, value: value.join('='), url: app.base, httpOnly: true, sameSite: 'Lax' }]);
  const pages = new Map(), errors = [];
  return {
    errors,
    async watch(item) {
      const page = await context.newPage();
      page.on('pageerror', () => errors.push('browser_page_error'));
      await page.goto(`${app.base}/w/${app.path.split('/').at(-1)}/?view=chat&conversation=${item.chat}`);
      await page.getByRole('textbox', { name: 'Message', exact: true }).waitFor();
      await page.getByRole('log', { name: 'Messages', exact: true }).waitFor();
      await page.evaluate(() => {
        const log = document.querySelector('[aria-label="Messages"]');
        const state = { first_visible_ms: null };
        window.calibrationTiming = state;
        const observer = new MutationObserver(() => {
          requestAnimationFrame(() => {
            const message = [...log.querySelectorAll('article')].find(node => node.querySelector('.chat-agent-label'));
            const text = message?.querySelector('.message-markdown');
            if (!state.first_visible_ms && text?.textContent?.trim() && text.getClientRects().length) {
              state.first_visible_ms = Date.now(); observer.disconnect();
            }
          });
        });
        observer.observe(log, { childList: true, subtree: true, characterData: true });
      });
      pages.set(item.marker, page);
    },
    async send(item) {
      const page = pages.get(item.marker);
      await page.getByRole('textbox', { name: 'Message', exact: true }).fill(item.content);
      const ack = page.waitForResponse(response => response.request().method() === 'POST'
        && new URL(response.url()).pathname === `${app.path}/chats/${item.chat}/messages`);
      const button = page.getByRole('button', { name: 'Send message', exact: true });
      await button.evaluate(node => node.addEventListener('click', () => {
        window.calibrationTiming.send_clicked_ms = Date.now();
      }, { once: true, capture: true }));
      item.driver_send_began_ms = Date.now();
      await button.click();
      const response = await ack;
      assert.equal(response.status(), 200);
      item.ack_ms = Date.now();
      item.sent_ms = await page.evaluate(() => window.calibrationTiming.send_clicked_ms);
      assert.ok(Number.isSafeInteger(item.sent_ms), 'Browser send click was not observed');
    },
    async visible(item, timeout) {
      const page = pages.get(item.marker);
      await page.waitForFunction(() => window.calibrationTiming?.first_visible_ms, null, { timeout });
      return page.evaluate(() => window.calibrationTiming.first_visible_ms);
    },
    async cancel(item) {
      await cancelRequest(pages.get(item.marker));
    },
    async capture(item, path) { await pages.get(item.marker).screenshot({ path }); },
    async closePage(item) {
      const page = pages.get(item.marker);
      if (page) { await page.close(); pages.delete(item.marker); }
    },
  };
}
