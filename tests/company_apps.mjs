// Company apps (Apps AP2/AP3) in the browser: an admin adds the company's order portal
// for chosen people with an owner; a member asks for access, the owner approves, and
// only then does Cuckoo open it. No model is involved.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { createRequire } from 'node:module';
import { workerFixture } from './fixtures/independent_worker.mjs';
const root = resolve(import.meta.dirname, '..');
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

test('Company apps open through Cuckoo only for people allowed; access is requested and approved', { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 120000 }, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true });
  const w = app.path.split('/').at(-1);
  const [ravi, mina] = [await app.login(1), await app.login(2)];
  const who = async cookie => (await app.request('/api/identity/session', cookie)).value.user;
  const raviUser = await who(ravi), minaUser = await who(mina);
  const browser = await chromium.launch({ headless: true }); const pages = []; const errors = [];
  t.after(async () => { if (!t.passed) for (const [i, page] of pages.entries()) { await page.screenshot({ path: join(app.data, `failure-${i}.png`) }).catch(() => {}); await writeFile(join(app.data, `failure-${i}.html`), await page.content()).catch(() => {}); } await browser.close(); });
  async function pageFor(cookie) { const context = await browser.newContext({ viewport: { width: 1280, height: 900 } }); const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]); const page = await context.newPage(); page.on('pageerror', e => errors.push(e.message)); pages.push(page); return page; }
  const open = async (cookie, id) => fetch(`${app.base}${app.path}/apps/systems/${id}/open`, { headers: { Cookie: cookie }, redirect: 'manual' });

  const adminPage = await pageFor(app.cookie);
  await adminPage.goto(`${app.base}/w/${w}/?view=apps`);
  await adminPage.getByRole('button', { name: 'Add app', exact: true }).click();
  await adminPage.getByRole('button', { name: /^Add your company's app/ }).click();
  const form = adminPage.getByRole('dialog', { name: "Add your company's app" });
  await form.getByLabel('Name', { exact: true }).fill('Order portal');
  await form.getByLabel('What it is for').fill('Orders and deliveries');
  await form.getByLabel('Address').fill('https://orders.example.test/home');
  await form.getByLabel('Who can open it').selectOption('selected');
  await form.getByLabel('Owner').selectOption(raviUser.id);
  await form.getByRole('button', { name: 'Add app', exact: true }).click();
  await form.waitFor({ state: 'hidden' });
  const portal = (await app.request(`${app.path}/apps`, app.cookie)).value.systems.find(s => s.name === 'Order portal');
  assert.equal(portal.audience, 'selected');
  app.pass('An admin adds the order portal for chosen people, with an owner who decides requests');

  const minaPage = await pageFor(mina);
  await minaPage.goto(`${app.base}/w/${w}/?view=apps`);
  await minaPage.getByText('No access', { exact: true }).waitFor();
  assert.equal((await open(mina, portal.id)).status, 404, 'Cuckoo refuses to open it for Mina');
  await minaPage.getByRole('button', { name: 'Request access', exact: true }).click();
  const ask = minaPage.getByRole('dialog', { name: 'Request access to Order portal' });
  await ask.getByLabel('Why').fill('I handle deliveries');
  await ask.getByRole('button', { name: 'Send request', exact: true }).click();
  await minaPage.getByText(`You asked for access. ${raviUser.display_name} decides.`, { exact: true }).waitFor();
  await minaPage.screenshot({ path: join(app.data, 'company-app-requested.png') });
  await minaPage.setViewportSize({ width: 390, height: 844 });
  await minaPage.screenshot({ path: join(app.data, 'company-app-mobile.png') });
  assert.equal(await minaPage.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  app.pass('Mina sees the app without its address, cannot open it, and asks for access; 390 px without overflow');

  const raviPage = await pageFor(ravi);
  await raviPage.goto(`${app.base}/w/${w}/?view=apps`);
  await raviPage.getByRole('button', { name: '1 request', exact: true }).click();
  const requests = raviPage.getByRole('dialog', { name: 'Requests' });
  await requests.getByText('“I handle deliveries”', { exact: true }).waitFor();
  await raviPage.screenshot({ path: join(app.data, 'company-app-request.png') });
  await requests.getByRole('button', { name: 'Approve', exact: true }).click();
  await requests.getByText('Nothing waiting.', { exact: true }).waitFor();
  const opened = await open(mina, portal.id);
  assert.equal(opened.status, 303);
  assert.equal(opened.headers.get('location'), 'https://orders.example.test/home');
  await minaPage.setViewportSize({ width: 1280, height: 900 });
  await minaPage.reload();
  await minaPage.getByRole('link', { name: 'Open Order portal', exact: true }).waitFor();
  await minaPage.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });
  await minaPage.screenshot({ path: join(app.data, 'company-app-dark.png') });
  const audit = (await app.request(`${app.path}/activity?kind=all`, app.cookie)).value.activity.map(e => e.sentence).join('\n');
  assert.match(audit, /Opened "Order portal"/);
  assert.match(audit, /request to open "Order portal"/);
  assert.deepEqual(errors, []);
  app.pass('The owner approves; Cuckoo then opens the portal for the member, and openings are audited; dark mode');
  void minaUser;
});
