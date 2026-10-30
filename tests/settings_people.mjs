// Settings in a real browser: the sidebar for admins and members, the People row menu,
// removal and leaving with hand-over (D231/D232), Audit log sentences, phone and dark layouts.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { root } from './fixtures/polling.mjs';

const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');

test('admins manage people from the Settings sidebar; removal and leaving hand shared work to the chosen person', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 240000,
}, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, apiEnv: { AYATI_PLATFORM_ADMINS: 'employee0@company.example.test' } });
  const [founder, leaving, heir] = [await app.login(3), await app.login(5), await app.login(6)];
  const me = async cookie => (await app.request('/api/identity/session', cookie)).value.user;
  const [owner, leaver, taker] = [await me(founder), await me(leaving), await me(heir)];
  const label = person => person.display_name || person.email;
  const code = (await app.request('/api/identity/admin/workspace-codes', app.cookie, { note: 'Settings test', days: 7 })).value.code;
  const w = (await app.request('/api/identity/workspaces', founder, { name: 'Mehta Furnishings', request_id: randomUUID(), code })).value.id;
  app.sql(`INSERT INTO relay_members(workspace,user_id,role) VALUES('${w}','${leaver.id}','Member'),('${w}','${taker.id}','Member')`);
  // The leaving member owns a shared agent and looks after a page shared with everyone.
  const agent = randomUUID();
  await app.request(`/api/identity/workspaces/${w}/agent-profiles`, founder, { id: agent, name: 'Atlas', role: 'Orders assistant', model: 'fixture', scope: 'workspace', revision: 0, settings: { provider: 'openai' } });
  app.sql(`UPDATE relay_shared_agents SET owner_id='${leaver.id}' WHERE workspace='${w}' AND id='${agent}'`);
  app.sql(`INSERT INTO relay_library_items(workspace,id,kind,title,created_by,steward,visibility,document,text,created,updated) VALUES('${w}','price-list','page','Price list','${leaver.id}','${leaver.id}','workspace','{}'::jsonb,'',1,1)`);

  const browser = await chromium.launch({ headless: true });
  const errors = [];
  t.after(() => browser.close());
  const open = async (cookie, viewport = { width: 1440, height: 900 }, colorScheme = 'light') => {
    const context = await browser.newContext({ viewport, colorScheme });
    const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
    const page = await context.newPage(); page.on('pageerror', error => errors.push(error.message)); return page;
  };
  const dir = process.env.SETTINGS_SHOTS ?? app.data;
  const shot = async (page, name) => { await page.waitForLoadState('networkidle'); await page.screenshot({ path: join(dir, `settings-${name}.png`) }); };
  const settings = `${app.base}/w/${w}/?view=settings`;

  // Admin: the sidebar and People.
  const admin = await open(founder);
  await admin.goto(`${settings}&section=people`);
  await admin.getByRole('heading', { name: 'People', exact: true }).waitFor();
  const items = await admin.locator('.settings-group button').allTextContents();
  assert.deepEqual(items, ['Profile', 'Preferences', 'Help', 'Workspace', 'People', 'Connections', 'UsageComing later', 'BillingComing later', 'Audit log']);
  await shot(admin, 'admin-people');

  // A role change asks first; Cancel changes nothing.
  await admin.getByRole('button', { name: `Actions for ${label(leaver)}` }).click();
  await shot(admin, 'row-menu');
  await admin.getByRole('menuitem', { name: 'Make admin' }).click();
  await admin.getByRole('dialog', { name: `Make ${label(leaver)} an admin?` }).getByRole('button', { name: 'Cancel' }).click();
  assert.equal(app.sql(`SELECT role FROM relay_members WHERE workspace='${w}' AND user_id='${leaver.id}'`), 'Member');

  // Removal shows what moves, lets the admin choose who takes over, then hands it on.
  await admin.getByRole('button', { name: `Actions for ${label(leaver)}` }).click();
  await admin.getByRole('menuitem', { name: 'Remove from workspace…' }).click();
  const dialog = admin.getByRole('dialog', { name: `Remove ${label(leaver)}?` });
  await dialog.getByText('1 shared agent: Atlas').waitFor();
  await dialog.getByText('1 Library item shared with everyone').waitFor();
  await dialog.getByLabel('Who takes over').selectOption(taker.id);
  await dialog.getByText(`Moves to ${label(taker)}`).waitFor();
  await shot(admin, 'remove-dialog');
  await dialog.getByRole('button', { name: `Remove ${label(leaver)}` }).click();
  await admin.getByText(`${label(leaver)} was removed.`, { exact: false }).waitFor();
  assert.equal(app.sql(`SELECT count(*) FROM relay_members WHERE workspace='${w}' AND user_id='${leaver.id}'`), '0');
  assert.equal(app.sql(`SELECT owner_id FROM relay_shared_agents WHERE workspace='${w}' AND id='${agent}'`), taker.id);
  assert.equal(app.sql(`SELECT steward FROM relay_library_items WHERE workspace='${w}' AND id='price-list'`), taker.id);

  // The Audit log shows one readable sentence.
  await admin.getByRole('button', { name: 'Audit log' }).click();
  await admin.getByRole('heading', { name: 'Audit log' }).waitFor();
  await admin.getByText(`removed ${label(leaver)}; 1 agent and 1 Library item moved to ${label(taker)}`, { exact: false }).waitFor();
  await shot(admin, 'audit-log');

  // A member sees only their pages and who the admins are.
  const member = await open(heir);
  await member.goto(settings);
  assert.deepEqual(await member.locator('.settings-group button').allTextContents(), ['Profile', 'Preferences', 'Help', 'This workspace']);
  await member.getByRole('button', { name: 'This workspace' }).click();
  await member.locator('.settings-admins').getByText(label(owner)).waitFor();
  await member.getByText('You are a Member · 2 people', { exact: false }).waitFor();
  await shot(member, 'member-this-workspace');
  await member.goto(`${settings}&section=people`);
  await member.getByRole('heading', { name: 'Profile' }).waitFor();
  assert.equal(await member.getByRole('heading', { name: 'People', exact: true }).count(), 0);

  // The member leaves on their own; an admin takes over what was handed to them above.
  await member.goto(`${settings}&section=this-workspace`);
  await member.getByRole('button', { name: 'Leave workspace' }).click();
  const leave = member.getByRole('dialog', { name: 'Leave Mehta Furnishings?' });
  await leave.getByText(`Moves to ${label(owner)}`).waitFor();
  await leave.getByText('1 shared agent: Atlas').waitFor();
  await shot(member, 'leave-dialog');
  await leave.getByRole('button', { name: 'Leave workspace' }).click();
  await member.waitForURL(/view=account&choose=1/);
  assert.equal(app.sql(`SELECT count(*) FROM relay_members WHERE workspace='${w}' AND user_id='${taker.id}'`), '0');
  assert.equal(app.sql(`SELECT owner_id FROM relay_shared_agents WHERE workspace='${w}' AND id='${agent}'`), owner.id);
  assert.equal(app.sql(`SELECT steward FROM relay_library_items WHERE workspace='${w}' AND id='price-list'`), owner.id);
  await admin.reload();
  await admin.getByText(`${label(taker)} left the workspace; 1 agent and 1 Library item moved to ${label(owner)}`).waitFor();

  // Phone: the list first, then a page with a way back, without sideways scrolling.
  const phone = await open(founder, { width: 390, height: 844 });
  await phone.goto(settings);
  await phone.getByRole('button', { name: 'People' }).waitFor();
  await shot(phone, 'phone-list');
  await phone.getByRole('button', { name: 'People' }).click();
  await phone.getByRole('heading', { name: 'People', exact: true }).waitFor();
  await phone.getByText(label(owner)).first().waitFor();
  assert.equal(await phone.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  await shot(phone, 'phone-people');
  await phone.getByRole('button', { name: '‹ Settings' }).click();
  await phone.getByRole('button', { name: 'Audit log' }).waitFor();

  // Dark theme.
  const dark = await open(founder, { width: 1440, height: 900 }, 'dark');
  await dark.goto(`${settings}&section=people`);
  await dark.getByRole('heading', { name: 'People', exact: true }).waitFor();
  await shot(dark, 'dark-people');

  assert.deepEqual(errors, []);
  app.pass('Admins use the Settings sidebar and People menu; removal and leaving hand shared work on and are recorded; members, phones and dark mode work');
});
