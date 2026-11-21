import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const picture = (id, checksum = `${id}-hash`) => ({ id, name: `${id}.png`, mime: 'image/png', size: 2048, checksum });
const source = read => ({ read, href: path => path, response: id => `/messages/${id}/response`, file: id => `/files/${id}` });
const gallery = ['first', 'second', 'third'].map((id, index) => ({ id, name: `${id}.png`, by: index === 1 ? 'You' : 'Meera Iyer', at: 1786919506 + index }));

/** Gives the viewer a stage of a known size, which jsdom cannot measure, and counts live observers. */
async function withStage(size, run) {
  const observers = new Set();
  globalThis.ResizeObserver = class {
    constructor(notify) { this.notify = notify; }
    observe() { observers.add(this); this.notify([{ contentRect: size }]); }
    disconnect() { observers.delete(this); }
  };
  try { await run(observers); } finally { delete globalThis.ResizeObserver; }
}
/** jsdom does not decode images, so a test says how large the picture is and that it loaded. */
const load = (update, width, height) => update(() => {
  const img = document.querySelector('.image-viewer img');
  Object.defineProperty(img, 'naturalWidth', { value: width, configurable: true });
  Object.defineProperty(img, 'naturalHeight', { value: height, configurable: true });
  img.dispatchEvent(new window.Event('load'));
});
const level = () => document.querySelector('.image-viewer-level').textContent;
const key = (update, name) => update(() => document.querySelector('.image-viewer').dispatchEvent(new window.KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true })));

test('zoom levels start from the fitted size, never enlarge a small picture and keep the picture over the stage', async () => {
  const { clampOffset, fitScale, nextView, shownView, stepScale, zoomAt } = await import('../src/react/results/image-zoom.ts');
  const image = { width: 2000, height: 1000 }, stage = { width: 1000, height: 800 };
  assert.equal(fitScale(image, stage), 0.5);
  assert.equal(fitScale({ width: 300, height: 200 }, stage), 1);
  assert.equal(fitScale(image, { width: 0, height: 0 }), 1);
  // From a fitted 50% the next level is 75%, not 125% of the real size.
  assert.equal(stepScale(0.5, 0.5, 1), 0.75); assert.equal(stepScale(0.75, 0.5, 1), 1); assert.equal(stepScale(4, 0.5, 1), 4);
  assert.equal(stepScale(1, 0.5, -1), 0.75); assert.equal(stepScale(0.75, 0.5, -1), 0.5); assert.equal(stepScale(0.5, 0.5, -1), 0.5);
  // A level a hair above the fitted size is skipped.
  assert.equal(stepScale(0.48, 0.48, 1), 0.75);
  assert.deepEqual(clampOffset({ x: 900, y: -900 }, image, stage, 1), { x: 500, y: -100 });
  assert.deepEqual(clampOffset({ x: 40, y: 40 }, image, stage, 0.5), { x: 0, y: 0 });
  // The point under the pointer (200, 100 from the centre) is still there after doubling.
  assert.deepEqual(zoomAt({ x: 0, y: 0 }, 0.5, 1, { x: 200, y: 100 }), { x: -200, y: -100 });
  assert.equal(nextView({ scale: 0.3, offset: { x: 9, y: 9 } }, image, stage), undefined);
  assert.deepEqual(nextView({ scale: 9, offset: { x: 0, y: 0 } }, image, stage), { scale: 4, offset: { x: 0, y: 0 } });
  // A stored zoom that a larger window now fits falls back to the fitted view.
  assert.deepEqual(shownView({ scale: 0.75, offset: { x: 50, y: 0 } }, image, { width: 4000, height: 2000 }), { scale: 1, offset: { x: 0, y: 0 } });
});

test('image viewer loads full bytes only for the opened picture, zooms from the fitted size and offers the exact original', async () => {
  const calls = [];
  await withStage({ width: 1000, height: 500 }, () => reactFixture(async ({ render, button, click, update }) => {
    const { ImageViewer } = await import('../src/react/results/image-viewer.tsx');
    await render(ImageViewer, { id: 'second', name: 'second.png', images: gallery, source: source(async path => { calls.push(path); return picture('second'); }), close: () => {} });
    assert.ok(calls.length > 0 && calls.every(path => path.startsWith('/files/second?') && path.includes('metadata=true')));
    const viewer = document.querySelector('dialog.image-viewer');
    assert.equal(viewer.getAttribute('aria-label'), 'second.png'); assert.equal(viewer.open, true);
    assert.equal(document.querySelectorAll('.image-viewer img').length, 1);
    assert.match(document.querySelector('.image-viewer img').getAttribute('src'), /^\/files\/second\?inline=true&version=second-hash$/);
    assert.match(document.querySelector('a[download]').getAttribute('href'), /^\/files\/second\?version=second-hash$/);
    assert.equal(document.querySelector('a[download]').getAttribute('aria-label'), 'Download original');
    assert.match(document.querySelector('.image-viewer-sender').textContent, /^You/);
    assert.match(document.querySelector('.image-viewer-file').textContent, /second\.png · 2 KB/);
    assert.equal(document.querySelector('.image-viewer-foot').textContent, '2 of 3');
    assert.equal(level(), 'Fit');
    await load(update, 2000, 1000);
    assert.equal(level(), '50%'); assert.equal(button('Zoom out').getAttribute('aria-disabled'), 'true');
    assert.equal(document.querySelector('.image-viewer img').style.width, '1000px');
    await click(button('Zoom in'));
    assert.equal(level(), '75%'); assert.equal(document.querySelector('.image-viewer img').style.width, '1500px');
    assert.match(document.querySelector('.image-viewer-foot').textContent, /Drag to move/);
    await key(update, '+'); assert.equal(level(), '100%');
    await key(update, '-'); assert.equal(level(), '75%');
    await click(document.querySelector('.image-viewer-level')); assert.equal(level(), '50%');
    await click(button('Zoom out')); assert.equal(level(), '50%');
    for (let step = 0; step < 8; step++) await click(button('Zoom in'));
    assert.equal(level(), '400%'); assert.equal(button('Zoom in').getAttribute('aria-disabled'), 'true');
    await key(update, '0'); assert.equal(level(), '50%');
  }));
});

test('arrows move between the conversation\'s pictures and a late read for the previous one cannot replace the current one', async () => {
  const pending = {};
  const read = path => path.includes('/files/first?') ? new Promise(resolve => { pending.first = resolve; }) : Promise.resolve(picture(path.match(/\/files\/(\w+)/)[1]));
  await withStage({ width: 1000, height: 500 }, () => reactFixture(async ({ render, button, click, update }) => {
    const { ImageViewer } = await import('../src/react/results/image-viewer.tsx');
    await render(ImageViewer, { id: 'first', name: 'first.png', images: gallery, source: source(read), close: () => {} });
    assert.equal(document.querySelector('.image-viewer img'), null);
    assert.match(document.querySelector('.image-viewer-stage').textContent, /Loading image/);
    assert.equal(button('Previous image').getAttribute('aria-disabled'), 'true');
    await click(button('Next image'));
    assert.match(document.querySelector('.image-viewer img').getAttribute('src'), /\/files\/second\?inline=true&version=second-hash/);
    await update(() => pending.first(picture('first', 'PRIVATE_OLD_VERSION')));
    assert.equal(document.querySelectorAll('.image-viewer img').length, 1);
    assert.doesNotMatch(document.querySelector('.image-viewer').innerHTML, /PRIVATE_OLD_VERSION|files\/first/);
    assert.equal(document.querySelector('dialog.image-viewer').getAttribute('aria-label'), 'second.png');
    // A zoomed picture moves with the arrow keys; at the fitted size they change the picture.
    await load(update, 2000, 1000); await click(button('Zoom in'));
    await key(update, 'ArrowRight'); assert.equal(document.querySelector('.image-viewer-foot').textContent.includes('of'), false);
    assert.match(document.querySelector('.image-viewer img').getAttribute('src'), /files\/second/);
    await key(update, '0'); await key(update, 'ArrowRight');
    assert.match(document.querySelector('.image-viewer img').getAttribute('src'), /\/files\/third\?inline=true&version=third-hash/);
    assert.equal(level(), 'Fit'); assert.equal(document.querySelector('.image-viewer-foot').textContent, '3 of 3');
    assert.equal(button('Next image').getAttribute('aria-disabled'), 'true');
    await click(button('Next image')); await key(update, 'ArrowRight');
    assert.match(document.querySelector('.image-viewer img').getAttribute('src'), /files\/third/);
  }));
});

test('a picture outside the conversation list opens alone, and lost access clears it and stops watching on close', async () => {
  let check, denied = false, closed = 0;
  const adapter = { ...source(async () => { if (denied) throw Object.assign(new Error('Source access revoked'), { status: 403 }); return picture('solo'); }), subscribe: fn => { check = fn; return () => { check = undefined; }; } };
  await withStage({ width: 800, height: 600 }, async observers => {
    await reactFixture(async ({ render, button, click, update }) => {
      const { ImageViewer } = await import('../src/react/results/image-viewer.tsx');
      await render(ImageViewer, { id: 'solo', name: 'solo.png', images: gallery, source: adapter, close: () => { closed++; } });
      assert.equal(document.querySelector('.image-viewer-sender'), null);
      assert.equal(button('Next image'), undefined); assert.equal(document.querySelector('.image-viewer-foot').textContent, '');
      assert.match(document.querySelector('.image-viewer-file').textContent, /solo\.png/);
      assert.ok(document.querySelector('.image-viewer img'));
      // Access is rechecked only while the page is visible; jsdom starts hidden.
      Object.defineProperty(document, 'hidden', { value: false, configurable: true });
      denied = true; await update(() => check());
      assert.equal(document.querySelector('.image-viewer img'), null); assert.equal(document.querySelector('a[download]'), null);
      assert.match(document.querySelector('.image-viewer [role=alert]').textContent, /Source access revoked/);
      denied = false; await click(button('Retry'));
      assert.ok(document.querySelector('.image-viewer img'));
      await update(() => document.querySelector('.image-viewer img').dispatchEvent(new window.Event('error')));
      assert.equal(document.querySelector('.image-viewer img'), null);
      assert.match(document.querySelector('.image-viewer-stage').textContent, /Preview unavailable for this image/);
      assert.ok(document.querySelector('a[download]'));
      await click(button('Close')); assert.equal(closed, 1);
      await update(() => document.querySelector('dialog.image-viewer').dispatchEvent(new window.Event('cancel', { cancelable: true }))); assert.equal(closed, 2);
    });
    assert.equal(observers.size, 0);
  });
  assert.equal(check, undefined);
});

test('conversation pictures list PNG and JPEG once, oldest first, without deleted messages', async () => {
  const { conversationImages } = await import('../src/react/chat/conversation-images.ts');
  const file = (id, mime = 'image/png') => ({ id, name: `${id}.png`, mime, size: 1, checksum: id });
  const message = (id, author, kind, attachments, state = 'complete') => ({ id, author, kind, name: kind === 'agent' ? null : 'Meera Iyer', content: '', state, at: 100, attachments });
  const directory = { users: [{ id: 'meera', name: 'Meera Iyer' }], agents: [{ id: 'atlas', name: 'Atlas' }] };
  const images = conversationImages([
    message('m1', 'meera', 'user', [file('one'), file('notes', 'text/plain'), file('vector', 'image/svg+xml')]),
    message('m2', 'atlas', 'agent', [file('two', 'image/jpeg')]),
    message('m3', 'meera', 'user', [file('gone')], 'deleted'),
    message('m4', 'meera', 'user', [file('one'), file('three')]),
    message('m5', 'meera', 'user'),
  ], directory);
  assert.deepEqual(images.map(image => [image.id, image.by, image.at]), [['one', 'Meera Iyer', 100], ['two', 'Atlas', 100], ['three', 'Meera Iyer', 100]]);
  assert.ok(images.every(image => image.face));
});
