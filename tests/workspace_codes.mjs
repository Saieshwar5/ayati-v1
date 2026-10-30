// Invite-only early access in a real browser: a platform administrator issues a
// code, another person creates a workspace with it, and the code works only once.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { root } from './fixtures/polling.mjs';

const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');

test('a platform administrator issues a code that creates exactly one workspace', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000,
}, async t => {
  // Only employee 0 administers Cuckoo here; everyone else needs a code.
  const app = await workerFixture(t, { mode: 'pooled', control: true, apiEnv: { AYATI_PLATFORM_ADMINS: 'employee0@company.example.test' } });
  const [founder, other] = [await app.login(3), await app.login(4)];
  const browser = await chromium.launch({ headless: true });
  const errors = [];
  t.after(() => browser.close());
  const open = async (cookie, viewport = { width: 1440, height: 900 }) => {
    const context = await browser.newContext({ viewport });
    if (cookie) { const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]); }
    const page = await context.newPage(); page.on('pageerror', error => errors.push(error.message)); return page;
  };
  // Wait for the logo (a CSS mask image) to load before each screenshot.
  const shot = async (page, name) => { await page.waitForLoadState('networkidle'); await page.screenshot({ path: join(app.data, `access-${name}.png`) }); };

  // Without a code nobody else can create a workspace, and the admin routes refuse them.
  const refused = await app.request('/api/identity/workspaces', founder, { name: 'No code', request_id: randomUUID() }, 400);
  assert.match(refused.value.error, /invite code is required/);
  assert.equal((await app.request('/api/identity/admin/workspace-codes', founder, undefined, 403)).value.error, 'Only Cuckoo administrators can manage workspace codes');

  const visitor = await open(null);
  await visitor.goto(`${app.base}/?view=account`); await visitor.getByRole('heading', { name: 'Sign in' }).waitFor();
  await shot(visitor, 'sign-in');

  // The administrator issues a code; it is shown once.
  const admin = await open(app.cookie);
  await admin.goto(`${app.base}/?view=account&choose=1`);
  await admin.getByRole('link', { name: 'Cuckoo admin' }).click();
  await admin.getByLabel('For (company or person)').fill('Mehta Furnishings');
  await admin.getByRole('button', { name: 'Create code' }).click();
  const code = (await admin.locator('.access-issued code').textContent()).trim();
  assert.match(code, /^CKO-[A-Z2-9]{4}-[A-Z2-9]{4}-[A-Z2-9]{4}$/);
  await admin.getByRole('table').getByText('Unused').waitFor();
  await shot(admin, 'admin-codes');

  // The founder enters it, names the workspace and lands inside as its owner.
  const page = await open(founder);
  await page.goto(`${app.base}/?view=account&choose=1`); await page.getByRole('heading', { name: /Welcome back/ }).waitFor();
  await shot(page, 'choose-workspace');
  await page.getByRole('button', { name: 'Enter an invite code' }).click();
  await page.getByLabel('Invite code').fill('CKO-ZZZZ-ZZZZ-ZZZZ'); await page.getByRole('button', { name: 'Continue' }).click();
  await page.getByRole('alert').getByText('Code invalid or expired').waitFor();
  await page.getByLabel('Invite code').fill(code.toLowerCase()); await page.getByRole('button', { name: 'Continue' }).click();
  await page.getByRole('heading', { name: 'Set up your workspace' }).waitFor();
  await page.getByLabel('Workspace name').fill('Mehta Furnishings');
  await shot(page, 'create-workspace');
  await page.getByRole('button', { name: 'Create workspace' }).click();
  await page.waitForURL(/\/w\/[0-9a-f-]{36}\//);
  const workspace = page.url().match(/\/w\/([0-9a-f-]{36})\//)[1];
  const session = (await app.request('/api/identity/session', founder)).value;
  assert.equal(session.workspaces.find(w => w.id === workspace)?.role, 'Owner');

  // The code is used up: nobody can create a second workspace with it.
  const again = await app.request('/api/identity/workspaces', other, { name: 'Copy', request_id: randomUUID(), code }, 400);
  assert.equal(again.value.error, 'Code invalid or expired');
  await admin.reload(); await admin.getByRole('table').getByText('Used', { exact: true }).waitFor();
  assert.match(await admin.getByRole('table').textContent(), /Mehta Furnishings/);

  // Phone layouts.
  const phone = await open(founder, { width: 390, height: 844 });
  await phone.goto(`${app.base}/?view=account&choose=1`); await phone.getByRole('heading', { name: /Welcome back/ }).waitFor();
  await shot(phone, 'choose-phone');
  await phone.getByRole('button', { name: 'Enter an invite code' }).click(); await shot(phone, 'code-phone');
  assert.equal(await phone.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  const anonymousPhone = await open(null, { width: 390, height: 844 });
  await anonymousPhone.goto(`${app.base}/?view=account`); await anonymousPhone.getByRole('heading', { name: 'Sign in' }).waitFor();
  await shot(anonymousPhone, 'sign-in-phone');
  assert.deepEqual(errors, []);
  app.pass('Codes are issued by administrators only, create one workspace once, and every access page fits a phone');
});
