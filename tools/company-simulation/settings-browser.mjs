// Live Owner/User Settings, failed writes, and workspace-switch isolation.
import { chromium } from '../../web/node_modules/playwright-core/index.mjs';
import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { randomUUID } from 'node:crypto';

const { directory } = JSON.parse(await readFile(resolve(process.argv[2] ?? '.ayati-local/company-simulation/runs', 'latest.json')));
const c = JSON.parse(await readFile(join(directory, 'manifest.json')));
assert.equal(new URL(c.base).hostname, '127.0.0.1');
const browser = await chromium.launch({ headless: true, executablePath: '/usr/bin/chromium', args: ['--no-sandbox'] });
const headers = { Origin: c.base, 'X-Ayati-Request': 'identity' };
const errors = [], checks = [];
const base = `${c.base}/w/${c.w.split('/').at(-1)}/`;
async function session(number) {
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  assert.ok((await ctx.request.post(`${c.base}/api/identity/simulation/employees/${number}/login`, { headers, data: {} })).ok());
  const page = await ctx.newPage();
  page.on('pageerror', error => errors.push(String(error)));
  return { ctx, page };
}
const settings = (scope, section) => `${base}?view=settings&scope=${scope}&section=${section}`;
const check = async (name, action) => { await action(); checks.push(name); console.log(`PASS ${name}`); };
try {
  const owner = await session(0), user = await session(1);
  owner.page.on('dialog', dialog => dialog.accept());
  await check('Owner sees workspace controls; User keeps personal Settings and a quiet sidebar', async () => {
    await owner.page.goto(settings('workspace', 'people'));
    await owner.page.locator('.settings-people .ui-row').first().waitFor();
    assert.deepEqual(await owner.page.locator('nav[aria-label=Workspace] a').allTextContents(), ['Home', 'Apps', 'Chat', 'Agents', 'Settings']);
    await user.page.goto(settings('workspace', 'people'));
    await user.page.getByRole('heading', { name: 'My account', exact: true }).waitFor();
    assert.equal(await user.page.getByRole('tab', { name: 'Workspace', exact: true }).count(), 0);
    await owner.page.screenshot({ path: join(directory, 'settings-owner.png') });
    await user.page.screenshot({ path: join(directory, 'settings-user.png') });
  });
  await check('The last Owner cannot remove or demote themselves through the UI', async () => {
    await owner.page.getByRole('button', { name: `View ${c.employees[0].user.email}`, exact: true }).click();
    assert.equal(await owner.page.getByLabel('Workspace role', { exact: true }).isDisabled(), true);
    assert.equal(await owner.page.getByRole('button', { name: 'Remove from workspace', exact: true }).count(), 0);
    await owner.page.getByRole('button', { name: 'Close', exact: true }).click();
  });
  await check('A failed workspace save retains its draft; explicit retry succeeds', async () => {
    await owner.page.goto(settings('workspace', 'general'));
    const route = `**${c.w}/settings`;
    await owner.page.route(route, async route => {
      if (route.request().method() === 'POST') await route.fulfill({ status: 503, contentType: 'application/json', body: JSON.stringify({ error: 'Simulated save outage' }) });
      else await route.continue();
    });
    await owner.page.getByLabel('Workspace name').fill('Workspace audit');
    await owner.page.getByRole('button', { name: 'Save changes', exact: true }).click();
    await owner.page.getByRole('status').filter({ hasText: 'Simulated save outage' }).waitFor();
    assert.equal(await owner.page.getByLabel('Workspace name').inputValue(), 'Workspace audit');
    await owner.page.unroute(route);
    await owner.page.getByRole('button', { name: 'Save changes', exact: true }).click();
    await owner.page.getByRole('status').filter({ hasText: 'Workspace name saved.' }).waitFor();
    assert.equal(await owner.page.locator('.settings-context').textContent(), 'Workspace audit');
  });
  await check('A second open editor cannot overwrite a newer workspace name', async () => {
    const other = await owner.ctx.newPage();
    await other.goto(settings('workspace', 'general'));
    await other.getByLabel('Workspace name').waitFor();
    await owner.page.getByLabel('Workspace name').fill('Workspace audit updated');
    await owner.page.getByRole('button', { name: 'Save changes', exact: true }).click();
    await owner.page.getByRole('status').filter({ hasText: 'Workspace name saved.' }).waitFor();
    await other.getByLabel('Workspace name').fill('Stale draft');
    await other.getByRole('button', { name: 'Save changes', exact: true }).click();
    await other.getByRole('status').filter({ hasText: 'Workspace changed; reload before saving' }).waitFor();
    assert.equal(await other.getByLabel('Workspace name').inputValue(), 'Stale draft');
    assert.equal((await (await owner.ctx.request.get(c.base + c.w + '/settings')).json()).name, 'Workspace audit updated');
    await other.close();
  });
  await check('Unconfirmed invitation preserves the email draft and appears immediately for cancellation', async () => {
    const email = `browser-${randomUUID()}@example.test`;
    await owner.page.goto(settings('workspace', 'people'));
    await owner.page.getByRole('button', { name: '＋ Invite person', exact: true }).click();
    await owner.page.getByLabel('Email address', { exact: true }).fill(email);
    await owner.page.getByRole('button', { name: 'Send invitation', exact: true }).click();
    await owner.page.locator('dialog [role=status]').filter({ hasText: 'Email delivery could not be confirmed' }).waitFor();
    assert.equal(await owner.page.getByLabel('Email address', { exact: true }).inputValue(), email);
    await owner.page.getByRole('button', { name: 'Close', exact: true }).click();
    await owner.page.getByRole('tab', { name: 'Invitations', exact: true }).click();
    const row = owner.page.locator('.ui-row').filter({ hasText: email });
    await row.getByText('Delivery unconfirmed', { exact: true }).waitFor();
    await row.getByRole('button', { name: 'Cancel invitation', exact: true }).click();
    await owner.page.getByRole('status').filter({ hasText: 'Invitation cancelled.' }).waitFor();
    assert.equal(await row.count(), 0);
  });
  await check('Promotion reveals Owner settings; demotion removes an already open Owner view', async () => {
    await owner.page.getByRole('tab', { name: 'People', exact: true }).click();
    await owner.page.getByRole('button', { name: `View ${c.employees[1].user.email}`, exact: true }).click();
    await owner.page.getByLabel('Workspace role', { exact: true }).selectOption('Owner');
    await owner.page.getByRole('button', { name: 'Save role', exact: true }).click();
    await owner.page.getByRole('status').filter({ hasText: 'Workspace role updated.' }).waitFor();
    await owner.page.getByRole('button', { name: 'Close', exact: true }).click();
    await user.page.goto(settings('workspace', 'people'));
    await user.page.locator('.settings-people .ui-row').first().waitFor();
    assert.ok((await owner.ctx.request.post(c.base + c.w + '/members/' + c.employees[1].user.id, { headers, data: { role: 'User' } })).ok());
    await user.page.waitForURL(url => url.searchParams.get('view') === 'account' && url.searchParams.has('choose'), { timeout: 22000 });
    await user.page.goto(settings('workspace', 'people'));
    await user.page.getByRole('heading', { name: 'My account', exact: true }).waitFor();
    assert.equal(await user.page.getByRole('tab', { name: 'Workspace', exact: true }).count(), 0);
  });
  await check('Workspace switching clears old people and uses the role of the chosen workspace', async () => {
    const created = await user.ctx.request.post(c.base + '/api/identity/workspaces', { headers, data: { name: 'Browser solo workspace', request_id: randomUUID() } });
    assert.ok(created.ok()); const { id } = await created.json();
    await user.page.reload();
    await user.page.getByRole('button', { name: 'Your account', exact: true }).click();
    await user.page.getByRole('link', { name: 'Browser solo workspace', exact: true }).click();
    await user.page.goto(`${c.base}/w/${id}/?view=settings&scope=workspace&section=people`);
    await user.page.locator('.settings-people .ui-row').first().waitFor();
    assert.equal(await user.page.locator('.settings-people .ui-row').count(), 1);
    assert.equal(await user.page.getByRole('button', { name: `View ${c.employees[0].user.email}`, exact: true }).count(), 0);
    assert.equal(await user.page.getByRole('tab', { name: 'Workspace', exact: true }).count(), 1);
    await user.page.getByRole('button', { name: 'Your account', exact: true }).click();
    await user.page.locator(`a[href="/w/${c.w.split('/').at(-1)}/"]`).click();
    await user.page.goto(settings('workspace', 'people'));
    await user.page.getByRole('heading', { name: 'My account', exact: true }).waitFor();
    assert.equal(await user.page.getByRole('tab', { name: 'Workspace', exact: true }).count(), 0);
  });
  await check('Personal appearance persists; keyboard scopes and narrow/short layouts remain usable', async () => {
    await user.page.goto(settings('personal', 'appearance'));
    await user.page.getByRole('radio', { name: 'Dark', exact: true }).check();
    await user.page.reload(); await user.page.getByRole('radio', { name: 'Dark', exact: true }).waitFor();
    assert.equal(await user.page.evaluate(() => document.documentElement.dataset.theme), 'dark');
    await owner.page.goto(settings('workspace', 'people'));
    await owner.page.getByRole('tab', { name: 'Workspace', exact: true }).focus();
    await owner.page.keyboard.press('ArrowLeft');
    assert.equal(await owner.page.getByRole('tab', { name: 'Personal', exact: true }).getAttribute('aria-selected'), 'true');
    for (const [width, height] of [[1440, 560], [400, 700], [320, 568]]) {
      await owner.page.setViewportSize({ width, height });
      await owner.page.goto(settings('workspace', 'people'));
      await owner.page.locator('.settings-people .ui-row').first().waitFor();
      assert.equal(await owner.page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
      await owner.page.screenshot({ path: join(directory, `settings-${width}.png`) });
    }
  });
  assert.deepEqual(errors, []);
} catch (error) {
  checks.push({ failed: String(error) }); console.error(error); process.exitCode = 1;
} finally {
  await writeFile(join(directory, 'settings-browser-results.json'), JSON.stringify({ checks, errors }, null, 2));
  await browser.close();
}
