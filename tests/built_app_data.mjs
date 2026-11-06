// Built app data (Apps BA2) in the browser: a leave app keeps requests through the
// bridge. A member asks for days off and sees only their own requests; the builder
// gives a colleague the approver role on the Apps page; the approver sees every request
// and approves one; the draft preview's records stay apart from the live ones.
// No model is involved.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { createRequire } from 'node:module';
import { workerFixture } from './fixtures/independent_worker.mjs';
const root = resolve(import.meta.dirname, '..');
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

const manifest = JSON.stringify({
  name: 'Leave', roles: ['user', 'approver'],
  collections: { requests: { fields: { days: 'number', reason: 'text', approved: 'boolean' },
    access: { user: ['create', 'read_own'], approver: ['read', 'update'] } } },
});
const html = `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Leave</title></head>
<body><main><h1>Leave</h1><p id="who">…</p>
<form id="ask"><label>Days <input id="days" type="number" min="1" value="2"></label>
<label>Reason <input id="reason" value="Family trip"></label><button>Ask for leave</button></form>
<p id="status" role="status"></p><ul id="list"></ul></main><script src="app.js"></script></body></html>`;
const code = `let me;
const list = document.getElementById('list');
const status = document.getElementById('status');
async function show() {
  const { records } = await cuckoo.list('requests');
  list.replaceChildren(...records.map(r => {
    const li = document.createElement('li');
    li.textContent = r.by + ': ' + r.data.days + ' days, ' + r.data.reason + (r.data.approved ? ' · approved' : ' · waiting');
    if (me.roles.includes('approver') && !r.data.approved) {
      const b = document.createElement('button');
      b.textContent = 'Approve ' + r.by;
      b.onclick = async () => { await cuckoo.update('requests', r.id, { approved: true }, r.revision); show(); };
      li.append(' ', b);
    }
    return li;
  }));
}
document.getElementById('ask').onsubmit = async event => {
  event.preventDefault();
  try {
    await cuckoo.create('requests', { days: Number(document.getElementById('days').value), reason: document.getElementById('reason').value });
    status.textContent = 'Asked.';
    show();
  } catch (error) { status.textContent = error.message; }
};
cuckoo.me().then(person => { me = person; document.getElementById('who').textContent = person.name + ' (' + person.roles.join(', ') + ')'; show(); });
`;

test('Built apps keep data through the bridge, by the roles people hold', { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 120000 }, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true });
  const w = app.path.split('/').at(-1);
  const [ravi, mina, zoe] = [await app.login(1), await app.login(2), await app.login(3)];
  const who = async cookie => (await app.request('/api/identity/session', cookie)).value.user;
  const [minaUser, zoeUser] = [await who(mina), await who(zoe)];
  const browser = await chromium.launch({ headless: true }); const pages = []; const errors = [];
  t.after(async () => { if (!t.passed) for (const [i, page] of pages.entries()) { await page.screenshot({ path: join(app.data, `failure-${i}.png`) }).catch(() => {}); await writeFile(join(app.data, `failure-${i}.html`), await page.content()).catch(() => {}); } await browser.close(); });
  async function pageFor(cookie) { const context = await browser.newContext({ viewport: { width: 1280, height: 900 } }); const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]); const page = await context.newPage(); page.on('pageerror', e => errors.push(e.message)); pages.push(page); return page; }

  const id = crypto.randomUUID();
  const built = `${app.path}/apps/built/${id}`;
  await app.request(`${app.path}/apps/built`, ravi, { id, name: 'Leave', description: 'Ask for days off', audience: 'everyone', people: [] });
  let revision = 1;
  for (const [path, content] of [['cuckoo-app.json', manifest], ['index.html', html], ['app.js', code]]) {
    revision = (await app.request(`${built}/files`, ravi, { path, content, revision })).value.draft_revision;
  }
  const raviPage = await pageFor(ravi);
  await raviPage.goto(`${app.base}/w/${w}/?view=apps&run=${id}&preview=1`);
  const preview = raviPage.frameLocator('iframe[title="Leave"]');
  await preview.locator('#who').filter({ hasText: '(user, approver)' }).waitFor();
  await preview.getByRole('button', { name: 'Ask for leave' }).click();
  await preview.getByText('Asked.', { exact: true }).waitFor();
  assert.equal((await app.request(`${app.path}/apps`, mina)).value.built.length, 0);
  assert.equal((await app.request(`${built}/publish`, ravi, { note: 'First', revision })).value.version, 1);
  app.pass('The builder previews the draft with every role and keeps a preview record, then publishes');

  const minaPage = await pageFor(mina);
  await minaPage.goto(`${app.base}/w/${w}/?view=apps&run=${id}`);
  const minaApp = minaPage.frameLocator('iframe[title="Leave"]');
  await minaApp.locator('#who').filter({ hasText: `${minaUser.display_name} (user)` }).waitFor();
  assert.equal(await minaApp.locator('#list li').count(), 0, 'the preview record is not live data');
  await minaApp.locator('#days').fill('3');
  await minaApp.getByRole('button', { name: 'Ask for leave' }).click();
  await minaApp.getByText(`${minaUser.display_name}: 3 days, Family trip · waiting`).waitFor();
  const zoePage = await pageFor(zoe);
  await zoePage.goto(`${app.base}/w/${w}/?view=apps&run=${id}`);
  const zoeApp = zoePage.frameLocator('iframe[title="Leave"]');
  await zoeApp.locator('#who').filter({ hasText: '(user)' }).waitFor();
  await zoeApp.getByRole('button', { name: 'Ask for leave' }).click();
  await zoeApp.getByText(`${zoeUser.display_name}: 2 days, Family trip · waiting`).waitFor();
  assert.equal(await zoeApp.locator('#list li').count(), 1, 'Zoe sees only her own request');
  await minaPage.screenshot({ path: join(app.data, 'built-app-data-member.png') });
  app.pass('Members ask for leave through the bridge and each sees only their own requests');

  await raviPage.goto(`${app.base}/w/${w}/?view=apps`);
  await raviPage.getByRole('button', { name: 'Roles', exact: true }).click();
  const roles = raviPage.getByRole('dialog', { name: 'Roles in Leave' });
  await roles.getByText('Everyone who can open it holds "user".', { exact: false }).waitFor();
  await roles.getByLabel(zoeUser.display_name, { exact: true }).check();
  await roles.getByRole('button', { name: 'Save approver', exact: true }).click();
  await roles.getByText('Saved who holds "approver".', { exact: true }).waitFor();
  await raviPage.screenshot({ path: join(app.data, 'built-app-roles.png') });
  await zoePage.reload();
  await zoeApp.locator('#who').filter({ hasText: '(user, approver)' }).waitFor();
  await zoeApp.getByRole('button', { name: `Approve ${minaUser.display_name}` }).click();
  await zoeApp.getByText(`${minaUser.display_name}: 3 days, Family trip · approved`).waitFor();
  await minaPage.reload();
  await minaApp.getByText(`${minaUser.display_name}: 3 days, Family trip · approved`).waitFor();
  await zoePage.screenshot({ path: join(app.data, 'built-app-data-approver.png') });
  const denied = await fetch(`${app.base}${built}/data`, { method: 'POST', headers: { Cookie: mina, Origin: app.base, 'X-Ayati-Request': 'identity', 'Content-Type': 'application/json' },
    body: JSON.stringify({ op: 'list', collection: 'requests', preview: true }) });
  assert.equal(denied.status, 400, 'a member can\'t reach the preview data');
  const audit = (await app.request(`${app.path}/activity?kind=all`, app.cookie)).value.activity.map(e => e.sentence).join('\n');
  assert.match(audit, /Gave the role "approver" in "Leave" to 1 person/);
  assert.deepEqual(errors, []);
  app.pass('The builder makes Zoe an approver on the Apps page; she sees every request and approves Mina\'s; giving the role is audited');
});
