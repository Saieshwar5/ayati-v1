import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { act } from 'react';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Add app (designs/connector-screens 2): the server searches and pages the catalog.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=apps`;
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const tile = (id, name, extra = {}) => ({ id, name, description: name, category: 'email_calendar', kinds: ['read', 'change'], actions: 3, state: 'available', connection: null, ...extra });
const categories = [{ id: 'email_calendar', name: 'Email and calendar', count: 2 }, { id: 'accounting', name: 'Accounting and payments', count: 1 }];
const page = (apps, next_cursor = null) => ({ categories, apps, next_cursor, can_manage: true });
const pause = ms => act(async () => { await new Promise(resolve => setTimeout(resolve, ms)); });
const names = () => [...document.querySelectorAll('.catalog-tile strong')].map(node => node.textContent);
const search = path => Object.fromEntries(new URL(path, 'http://localhost').searchParams);
async function open(render, props = {}) {
  const { CatalogDialog } = await import('../src/react/apps/catalog-dialog.tsx');
  await render(CatalogDialog, { canManage: true, close() {}, connect() {}, ...props });
}

test('Search waits for a pause in typing, and an older answer never replaces a newer one', async () => {
  const held = new Map(), asked = [];
  await reactFixture(async ({ render, change, update }) => {
    await open(render);
    assert.deepEqual(names(), ['Gmail', 'Outlook']);
    assert.equal(document.querySelector('.catalog-search input').placeholder, 'Search 3 services');
    const input = document.querySelector('.catalog-search input');
    await change(input, 'd'); await change(input, 'dr');
    await pause(300);
    await change(input, 'sl');
    await pause(300);
    assert.deepEqual(asked.filter(query => query), ['dr', 'sl'], 'one request per pause, not per key');
    await update(() => held.get('sl')(page([tile('slack', 'Slack')])));
    await update(() => held.get('dr')(page([tile('drive', 'Google Drive')])));
    assert.deepEqual(names(), ['Slack'], 'the late answer for "dr" is ignored');
    await change(input, 'zz'); await pause(300);
    await update(() => held.get('zz')(page([])));
    assert.match(document.querySelector('.catalog-main').textContent, /No services match “zz”/);
  }, { url, fetch: async path => {
    const { query } = search(path); asked.push(query);
    if (!query) return response(page([tile('gmail', 'Gmail'), tile('outlook', 'Outlook')]));
    return new Promise(resolve => held.set(query, data => resolve(response(data))));
  } });
});

test('Load more asks once for the next page; a category starts over and drops a page for the old one', async () => {
  const asked = []; let release;
  await reactFixture(async ({ render, update, click, button }) => {
    await open(render);
    await update(() => { button('Load more').click(); button('Load more').click(); });
    assert.equal(asked.filter(query => query.cursor === 'p2').length, 1, 'one request per click burst');
    await update(() => release(page([tile('zoho', 'Zoho Books')])));
    assert.deepEqual(names(), ['Gmail', 'Outlook', 'Zoho Books']);
    assert.equal(button('Load more'), undefined, 'no more pages');
    await click([...document.querySelectorAll('.catalog-nav button')].find(node => node.textContent.startsWith('Accounting')));
    assert.deepEqual(asked.at(-1), { query: '', category: 'accounting' });
    assert.deepEqual(names(), ['Razorpay']);
    assert.equal([...document.querySelectorAll('.catalog-nav [aria-pressed=true]')].map(node => node.textContent).join(), 'Accounting and payments1');
    // A page that arrives after the category changed is dropped.
    await click(button('Load more'));
    await click([...document.querySelectorAll('.catalog-nav button')].find(node => node.textContent.startsWith('All services')));
    await update(() => release(page([tile('stale', 'Stale page')])));
    assert.deepEqual(names(), ['Gmail', 'Outlook']);
  }, { url, fetch: async path => {
    const query = search(path); asked.push(query);
    if (query.cursor) return new Promise(resolve => { release = data => resolve(response(data)); });
    if (query.category === 'accounting') return response(page([tile('razorpay', 'Razorpay', { category: 'accounting', kinds: ['read', 'money'] })], 'a2'));
    return response(page([tile('gmail', 'Gmail', { state: 'connected', connection: 'c1' }), tile('outlook', 'Outlook')], 'p2'));
  } });
});

test('Members request a service instead of connecting it; admins see Connect and the other ways to add', async () => {
  const sent = [];
  await reactFixture(async ({ render, click, button }) => {
    await open(render, { canManage: false, others: { suggest() {}, build() {} } });
    assert.equal(document.querySelector('dialog[open] h2').textContent, 'Request an app');
    assert.equal(button('Connect Outlook'), undefined);
    assert.match(document.querySelector('.catalog-tile:nth-child(2)').textContent, /RequestAdmins decide/);
    assert.match(document.querySelector('.catalog-tile:nth-child(3)').textContent, /RequestedYou asked/);
    assert.doesNotMatch(document.querySelector('.catalog-nav').textContent, /MCP server|Your company's app/);
    await click(button('Request Outlook'));
    assert.equal(document.querySelectorAll('dialog[open]')[1].querySelector('h2').textContent, 'Request Outlook');
    await click(button('Send request'));
    assert.deepEqual({ ...sent[0], id: undefined }, { id: undefined, name: 'Outlook', url: '', reason: '' });
    assert.match(sent[0].id, /^[0-9a-f-]{36}$/);
  }, { url, fetch: async (path, options = {}) => {
    if (options.body) { sent.push(JSON.parse(options.body)); return response({ id: 'r', state: 'pending' }); }
    return response({ ...page([tile('gmail', 'Gmail', { state: 'connected', connection: 'c1' }), tile('outlook', 'Outlook'), tile('slack', 'Slack', { state: 'requested' })]), can_manage: false });
  } });
  const chosen = [];
  await reactFixture(async ({ render, click, button }) => {
    await open(render, { connect: app => chosen.push(app.id), others: { addSystem() {}, build() {} } });
    assert.match(document.querySelector('.catalog-nav').textContent, /MCP serverNext.*Your company's app.*Build an app/);
    assert.match(document.querySelector('.catalog-tile:nth-child(2)').textContent, /Connect3 actions/);
    await click(button('Connect Outlook'));
    assert.deepEqual(chosen, ['outlook']);
  }, { url, fetch: async () => response(page([tile('gmail', 'Gmail', { state: 'connected', connection: 'c1' }), tile('outlook', 'Outlook')])) });
});
