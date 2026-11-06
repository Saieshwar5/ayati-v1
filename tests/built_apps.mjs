// Built apps (Apps BA1) in the browser: a member builds a small app from the starter,
// the checks stop a key in the code, they publish it for a chosen colleague, who opens it
// in a frame with no network and no access to Cuckoo's page; an admin takes it offline.
// No model is involved.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { createRequire } from 'node:module';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/polling.mjs';
const root = resolve(import.meta.dirname, '..');
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

const code = `const out = document.querySelector('main');
const say = (id, text) => out.insertAdjacentHTML('beforeend', '<p id="' + id + '">' + text + '</p>');
say('ready', 'Leave form ready.');
fetch('https://example.com/').then(() => say('net', 'network allowed'), () => say('net', 'network blocked'));
try { void parent.document.title; say('parent', 'parent readable'); } catch { say('parent', 'parent blocked'); }
try { say('cookie', document.cookie ? 'cookie readable' : 'cookie empty'); } catch { say('cookie', 'cookie blocked'); }
`;

test('Built apps publish after their checks and run walled off for the people chosen', { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 120000 }, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true });
  const w = app.path.split('/').at(-1);
  const [ravi, mina] = [await app.login(1), await app.login(2)];
  const who = async cookie => (await app.request('/api/identity/session', cookie)).value.user;
  const minaUser = await who(mina);
  const browser = await chromium.launch({ headless: true }); const pages = []; const errors = [];
  t.after(async () => { if (!t.passed) for (const [i, page] of pages.entries()) { await page.screenshot({ path: join(app.data, `failure-${i}.png`) }).catch(() => {}); await writeFile(join(app.data, `failure-${i}.html`), await page.content()).catch(() => {}); } await browser.close(); });
  async function pageFor(cookie) { const context = await browser.newContext({ viewport: { width: 1280, height: 900 } }); const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]); const page = await context.newPage(); page.on('pageerror', e => errors.push(e.message)); page.on('dialog', d => void d.accept()); pages.push(page); return page; }

  const raviPage = await pageFor(ravi);
  await raviPage.goto(`${app.base}/w/${w}/?view=apps`);
  await raviPage.getByRole('button', { name: 'Build an app', exact: true }).click();
  const form = raviPage.getByRole('dialog', { name: 'Build an app' });
  await form.getByLabel('Name', { exact: true }).fill('Leave requests');
  await form.getByLabel('What it is for').fill('Ask for time off');
  await form.getByLabel('Who can use it').selectOption('selected');
  await form.getByLabel(minaUser.display_name, { exact: true }).check();
  await form.getByRole('button', { name: 'Start building', exact: true }).click();
  await form.waitFor({ state: 'hidden' });
  await raviPage.getByText('Not published yet. Only its builder and admins see it.', { exact: true }).waitFor();
  const built = (await app.request(`${app.path}/apps`, ravi)).value.built[0];
  assert.equal(built.version, null);
  assert.deepEqual((await app.request(`${app.path}/apps`, mina)).value.built, [], 'a draft is invisible to Mina');
  app.pass('A member builds an app from the starter for a chosen colleague; the draft is theirs alone');

  await raviPage.getByRole('button', { name: 'Code and publish', exact: true }).click();
  const drawer = raviPage.getByRole('dialog', { name: 'Leave requests: code' });
  await drawer.getByRole('button', { name: 'app.js', exact: true }).click();
  await drawer.getByLabel('Contents of app.js').fill(code);
  await drawer.getByRole('button', { name: 'Save file', exact: true }).click();
  await drawer.getByText('Saved app.js.', { exact: true }).waitFor();
  await drawer.getByLabel('New file name').fill('config.js');
  await drawer.getByRole('button', { name: 'Add file', exact: true }).click();
  await drawer.getByText('Saved config.js.', { exact: true }).waitFor();
  await drawer.getByLabel('Contents of config.js').fill("const key = 'sk-abcdefghijklmnopqrstuvwxyz0123456789';");
  await drawer.getByRole('button', { name: 'Save file', exact: true }).click();
  await drawer.getByText('Saved config.js.', { exact: true }).waitFor();
  await drawer.getByLabel('What changed').fill('First version');
  await drawer.getByRole('button', { name: 'Publish', exact: true }).click();
  await drawer.getByText(/config\.js looks like it contains an API token/).waitFor();
  await raviPage.screenshot({ path: join(app.data, 'built-app-checks.png') });
  await drawer.getByRole('button', { name: 'Delete file', exact: true }).click();
  await drawer.getByText('Deleted config.js.', { exact: true }).waitFor();
  await drawer.getByText(/Every check passes/).waitFor();
  await drawer.getByRole('button', { name: 'Publish', exact: true }).click();
  await drawer.getByText('Published version 1. People you chose can open it now.', { exact: true }).waitFor();
  await drawer.getByText('Version 1 · live').waitFor();
  const zip = await fetch(`${app.base}${app.path}/apps/built/${built.id}/download?version=1`, { headers: { Cookie: ravi } });
  assert.equal(zip.headers.get('content-type'), 'application/zip');
  assert.equal(Buffer.from(await zip.arrayBuffer()).subarray(0, 2).toString(), 'PK');
  app.pass('A key in the code blocks publishing; once removed, version 1 is published and downloads as a ZIP');

  const minaPage = await pageFor(mina);
  await minaPage.goto(`${app.base}/w/${w}/?view=apps`);
  await minaPage.getByRole('link', { name: 'Open Leave requests', exact: true }).click();
  const frame = minaPage.frameLocator('iframe[title="Leave requests"]');
  await frame.locator('#ready').waitFor();
  await frame.locator('#net').waitFor();
  assert.equal(await frame.locator('#net').textContent(), 'network blocked');
  assert.equal(await frame.locator('#parent').textContent(), 'parent blocked');
  assert.notEqual(await frame.locator('#cookie').textContent(), 'cookie readable');
  await minaPage.getByText('Version 1', { exact: true }).waitFor();
  await minaPage.screenshot({ path: join(app.data, 'built-app-running.png') });
  await app.request(`${app.path}/apps/built/${built.id}/files`, mina, undefined, 400);
  await minaPage.setViewportSize({ width: 390, height: 844 });
  await minaPage.screenshot({ path: join(app.data, 'built-app-mobile.png') });
  assert.equal(await minaPage.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  app.pass('Mina opens it: the app runs with no network, no access to Cuckoo\'s page or cookie, and no code for her; 390 px');

  // A draft that throws in its preview: the builder (and their agent) see the error.
  const draft = (await app.request(`${app.path}/apps/built/${built.id}/files?version=0`, ravi)).value;
  await app.request(`${app.path}/apps/built/${built.id}/files`, ravi, { path: 'app.js', content: "throw new Error('Leave form broke');", revision: draft.draft_revision });
  await raviPage.goto(`${app.base}/w/${w}/?view=apps&run=${built.id}&preview=1`);
  await until(async () => (await app.request(`${app.path}/apps/built/${built.id}/files?version=0`, ravi)).value.preview_errors.length === 1, 'The preview error was not kept', 15000);
  await raviPage.goto(`${app.base}/w/${w}/?view=apps`);
  await raviPage.getByRole('button', { name: 'Code and publish', exact: true }).click();
  await raviPage.getByRole('region', { name: 'Preview errors' }).getByText(/Leave form broke/).waitFor();
  await raviPage.screenshot({ path: join(app.data, 'built-app-preview-error.png') });
  await raviPage.keyboard.press('Escape');
  app.pass('A draft that throws in its preview shows the error to its builder under "In the last preview"');

  const adminPage = await pageFor(app.cookie);
  await adminPage.goto(`${app.base}/w/${w}/?view=apps`);
  await adminPage.getByRole('button', { name: 'Code and versions', exact: true }).click();
  const admin = adminPage.getByRole('dialog', { name: 'Leave requests: code' });
  assert.equal(await admin.getByRole('button', { name: 'Publish', exact: true }).count(), 0, 'only the builder publishes');
  await admin.getByRole('button', { name: 'Take offline', exact: true }).click();
  await admin.getByText('The app is offline.', { exact: true }).waitFor();
  await minaPage.reload();
  await minaPage.getByText("This app isn't published for you.", { exact: true }).waitFor();
  await adminPage.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });
  await adminPage.screenshot({ path: join(app.data, 'built-app-dark.png') });
  const audit = (await app.request(`${app.path}/activity?kind=all`, app.cookie)).value.activity.map(e => e.sentence).join('\n');
  assert.match(audit, /Published "Leave requests" version 1/);
  assert.match(audit, /Opened "Leave requests"/);
  assert.match(audit, /Took "Leave requests" offline/);
  assert.deepEqual(errors.filter(e => e !== 'Leave form broke'), [], 'only the preview error raised on purpose');
  app.pass('An admin takes it offline from its versions; Mina can no longer open it; publishing and opening are audited; dark mode');
});
