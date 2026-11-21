import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';

// Pictures, files and voice in a message (designs/chat-attachments). Never pass DOM nodes to assert: compare values.
const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=chat`;
const session = async () => ({ ok: true, status: 200, json: async () => ({ user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] }) });
const picture = id => ({ id, name: `${id}.png`, mime: 'image/png', size: 610304, checksum: `${id}-sum` });
const pdf = (id, name, size = 376832) => ({ id, name, mime: 'application/pdf', size, checksum: `${id}-sum` });
// Every read answers with one stored file's details, which is all the file viewer asks for here.
const source = { read: async () => pdf('myth', '16. Myth of Sisyphus.pdf'), href: path => path, response: id => `/messages/${id}/response`, file: id => `/files/${id}` };
const labels = selector => [...document.querySelectorAll(selector)].map(node => node.getAttribute('aria-label'));

/** Shows a component inside the provider that opens files and pictures, as a conversation does. */
async function inConversation(render, Component, props) {
  const { Results } = await import('../src/react/results/provider.tsx');
  await render(() => createElement(Results, { source, changed: () => {} }, createElement(Component, props)));
}

test('a file says its type, size and name, and a long name keeps its ending', async () => {
  const { fileKind, fileSize, splitName } = await import('../src/react/results/file-kind.ts');
  assert.deepEqual(fileKind('16. Myth of Sisyphus.pdf', 'application/octet-stream'), { tile: 'PDF', label: 'PDF', tone: 'pdf' });
  assert.deepEqual(fileKind('Q3 pricing.XLSX', ''), { tile: 'XLS', label: 'Spreadsheet', tone: 'sheet' });
  assert.deepEqual(fileKind('Offer letter.docx', ''), { tile: 'DOC', label: 'Document', tone: 'doc' });
  assert.deepEqual(fileKind('site-photos.zip', 'application/zip'), { tile: 'ZIP', label: 'Archive' });
  assert.deepEqual(fileKind('scan', 'application/pdf'), { tile: 'PDF', label: 'PDF', tone: 'pdf' }, 'the type counts when the name has no ending');
  assert.deepEqual(fileKind('logo.svg', 'image/svg+xml'), { tile: 'IMG', label: 'Image' });
  assert.deepEqual(fileKind('model.step', 'application/octet-stream'), { tile: 'STEP', label: 'File' });
  assert.deepEqual(fileKind('.env', 'application/octet-stream'), { tile: 'FILE', label: 'File' });
  assert.deepEqual(fileKind('backup.sqlite3-journal', 'application/octet-stream'), { tile: 'FILE', label: 'File' });
  assert.deepEqual([fileSize(1), fileSize(26 * 1024), fileSize(1048575), fileSize(1048576), fileSize(8.4 * 1048576)], ['1 KB', '26 KB', '1024 KB', '1.0 MB', '8.4 MB']);
  assert.deepEqual(splitName('invoice.pdf'), { head: 'invoice.pdf', tail: '' });
  assert.deepEqual(splitName('046199498_Rel_18062024132753.pdf'), { head: '046199498_Rel_180620241', tail: '32753.pdf' });
});

test('a file is one row that opens it, with its own download; without a viewer the row downloads', async () => {
  await reactFixture(async ({ render, click }) => {
    const { MessageFiles } = await import('../src/react/results/files.tsx');
    const files = [pdf('myth', '16. Myth of Sisyphus.pdf'), pdf('rel', '046199498_Rel_18062024132753.pdf', 26 * 1024)];
    await inConversation(render, MessageFiles, { files });
    const rows = [...document.querySelectorAll('.chat-file-row')];
    assert.deepEqual(rows.map(row => row.querySelector('.chat-file-tile').textContent), ['PDF', 'PDF']);
    assert.deepEqual(rows.map(row => row.querySelector('.chat-file-name').textContent), files.map(file => file.name), 'the whole name is in the page');
    assert.deepEqual(rows.map(row => row.querySelector('small').textContent), ['PDF · 368 KB', 'PDF · 26 KB']);
    assert.deepEqual(labels('.chat-file-download'), ['Download 16. Myth of Sisyphus.pdf', 'Download 046199498_Rel_18062024132753.pdf']);
    assert.match(document.querySelector('.chat-file-download').getAttribute('href'), /\/objects\/myth\?version=myth-sum$/);
    assert.equal(document.querySelector('dialog'), null);
    await click(rows[0].querySelector('button.chat-file-open'));
    assert.equal(document.querySelectorAll('dialog').length, 1, 'the row opens the file');

    await render(MessageFiles, { files: [files[0], picture('shot')] });
    assert.equal(document.querySelector('button.chat-file-open'), null);
    assert.deepEqual(labels('a.chat-file-open'), ['Download 16. Myth of Sisyphus.pdf', 'Download shot.png'], 'a picture is a file where nothing can open it');
    assert.equal(document.querySelectorAll('a[download]').length, 2, 'one download per row');
  }, { url });
});

test('pictures show as pictures: one large, or four in a grid with the rest counted, each opening the viewer', async () => {
  await reactFixture(async ({ render, click, update }) => {
    const { MessagePictures, MessageFiles } = await import('../src/react/results/files.tsx');
    const both = props => createElement('div', null, createElement(MessagePictures, props), createElement(MessageFiles, props));
    await inConversation(render, both, { files: [picture('shot'), pdf('myth', 'Myth.pdf')] });
    assert.equal(document.querySelector('.chat-pictures').className, 'chat-pictures chat-pictures-1');
    assert.deepEqual(labels('.chat-picture'), ['Open shot.png']);
    assert.match(document.querySelector('.chat-picture img').getAttribute('src'), /\/objects\/shot\?thumbnail=true&version=shot-sum$/);
    assert.equal(document.querySelectorAll('.chat-file-row').length, 1, 'the picture has no file row, name or size under it');
    assert.equal(document.body.textContent.includes('shot.png'), false);
    // A preview that cannot be drawn shows the picture's name in its place and still opens.
    await update(() => document.querySelector('.chat-picture img').dispatchEvent(new window.Event('error')));
    assert.equal(document.querySelector('.chat-picture-missing').textContent, 'shot.png');

    const seven = ['a', 'b', 'c', 'd', 'e', 'f', 'g'].map(picture);
    await inConversation(render, both, { files: seven });
    assert.equal(document.querySelector('.chat-pictures').className, 'chat-pictures chat-pictures-4');
    assert.deepEqual(labels('.chat-picture'), ['Open a.png', 'Open b.png', 'Open c.png', 'Open d.png and 3 more pictures']);
    assert.equal(document.querySelector('.chat-picture-more').textContent, '+3');
    assert.equal(document.querySelector('.result-files'), null);
    await click(document.querySelectorAll('.chat-picture')[3]);
    assert.equal(document.querySelector('dialog.image-viewer').getAttribute('aria-label'), 'd.png');

    await inConversation(render, both, { files: seven.slice(0, 5) });
    assert.equal(labels('.chat-picture')[3], 'Open d.png and 1 more picture');
    await inConversation(render, both, { files: seven.slice(0, 3) });
    assert.equal(document.querySelector('.chat-pictures').className, 'chat-pictures chat-pictures-3');
    assert.equal(document.querySelector('.chat-picture-more'), null);
  }, { url });
});

test('a person’s pictures lead their message and their files follow their words', async () => {
  await reactFixture(async ({ render }) => {
    globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { ChatMessage } = await import('../src/react/chat/message.tsx');
    const directory = { users: [{ id: 'owner', name: 'Priya' }, { id: 'meera', name: 'Meera Iyer' }], agents: [], role: 'Admin' };
    const message = { id: 'm1', chat: 'room', author: 'owner', kind: 'user', name: 'Priya', content: 'Shared files', state: 'sent', at: 1, attachments: [picture('shot')] };
    const order = () => [...document.querySelector('article > div').children].map(node => node.className.split(' ')[0]);
    await inConversation(render, ChatMessage, { message, directory });
    // Only pictures: nothing but them and the time, which the stylesheet lays over the picture.
    assert.deepEqual(order(), ['chat-pictures', 'chat-message-meta']);
    assert.equal(document.body.textContent.includes('Shared files'), false);
    await inConversation(render, ChatMessage, { message: { ...message, content: 'What is going on here?', attachments: [pdf('myth', 'Myth.pdf'), picture('shot')] }, directory });
    assert.equal(order()[0], 'chat-pictures');
    assert.equal(order().at(-2), 'result-files');
    assert.equal(order().at(-1), 'chat-message-meta');
    assert.match(document.querySelector('article > div').textContent, /^What is going on here\?\s*PDFMyth\.pdf/);
    // Someone else's: the same order inside their bubble.
    await inConversation(render, ChatMessage, { message: { ...message, author: 'meera', name: 'Meera Iyer', content: 'Look' }, directory });
    assert.equal(document.querySelector('.chat-message-bubble').firstElementChild.className, 'chat-pictures chat-pictures-1');
    delete globalThis.IntersectionObserver;
  }, { url, fetch: session });
});

test('files waiting in the message box show as a preview or a tile, each with its own remove, and an upload shows in place', async () => {
  let finish;
  const fetch = async (path, options) => {
    if (options?.method !== 'POST') return session();
    await new Promise(resolve => { finish = resolve; });
    return { ok: true, status: 200, json: async () => pdf('stored', 'policy.pdf', 2048) };
  };
  await reactFixture(async ({ render, click, update, button }) => {
    const { useState } = await import('react');
    const { ChatComposer } = await import('../src/react/chat/attachments.tsx');
    function Harness() {
      const [files, setFiles] = useState([picture('shot'), pdf('myth', '16. Myth of Sisyphus.pdf')]);
      return createElement(ChatComposer, { chat: 'room', value: '', change: () => {}, files, setFiles, send: async () => true });
    }
    await render(Harness);
    assert.deepEqual(labels('.composer-file-remove'), ['Remove shot.png', 'Remove 16. Myth of Sisyphus.pdf']);
    assert.equal(document.querySelector('.composer-file.is-picture img').getAttribute('alt'), 'shot.png');
    const tile = document.querySelectorAll('.composer-file')[1];
    assert.equal(tile.textContent, 'PDF16. Myth of Sisyphus.pdf368 KB×');

    const picker = document.querySelector('input[type=file]');
    Object.defineProperty(picker, 'files', { value: [new window.File(['policy'], 'policy.pdf', { type: 'application/pdf' })], configurable: true });
    await update(() => picker.dispatchEvent(new window.Event('change', { bubbles: true })));
    assert.equal(document.querySelector('.composer-file.is-uploading').textContent, 'policy.pdfUploading…');
    assert.equal(button('Remove shot.png').disabled, true, 'nothing is removed while a file is being stored');
    await update(() => finish());
    assert.equal(document.querySelector('.is-uploading'), null);
    assert.deepEqual(labels('.composer-file-remove'), ['Remove shot.png', 'Remove 16. Myth of Sisyphus.pdf', 'Remove policy.pdf']);
    await click(button('Remove shot.png'));
    assert.deepEqual(labels('.composer-file-remove'), ['Remove 16. Myth of Sisyphus.pdf', 'Remove policy.pdf']);
  }, { url, fetch });
});
