import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';

// The sent voice message player (D245). Never pass DOM nodes to assert: compare values.
const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=chat`;
const session = async () => ({ ok: true, status: 200, json: async () => ({ user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] }) });
const voice = (id, extra) => ({ id, name: 'voice-message.webm', mime: 'audio/webm', size: 4096, checksum: `${id}-sum`, transcript_status: 'not_enabled', ...extra });

/** jsdom has no media playback: a minimal element that plays, pauses and seeks. */
function playableMedia(dom) {
  const proto = dom.window.HTMLMediaElement.prototype;
  Object.defineProperty(proto, 'paused', { configurable: true, get() { return this._paused ?? true; } });
  Object.defineProperty(proto, 'currentTime', { configurable: true, get() { return this._time ?? 0; }, set(value) { this._time = value; } });
  proto.play = function () { this._paused = false; this.dispatchEvent(new dom.window.Event('play')); return Promise.resolve(); };
  proto.pause = function () { if (this._paused === false) { this._paused = true; this.dispatchEvent(new dom.window.Event('pause')); } };
}
const players = () => [...document.querySelectorAll('.voice-play')].map(node => node.getAttribute('aria-label'));
const slider = () => document.querySelector('[role=slider]');
const key = (dom, name) => slider().dispatchEvent(new dom.window.KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true }));

test('only one voice message plays at a time, and leaving stops it', async () => {
  await reactFixture(async ({ render, click, button, dom }) => {
    playableMedia(dom);
    const { MessageFiles } = await import('../src/react/results/files.tsx');
    await render(MessageFiles, { files: [voice('first', { duration_ms: 38_000 }), voice('second', { duration_ms: 12_000 })] });
    assert.equal(document.querySelectorAll('audio[controls]').length, 0, 'no raw browser controls');
    assert.equal(document.querySelectorAll('.chat-file button').length, 2, 'voice notes show no file-name row');
    const [first, second] = document.querySelectorAll('.voice-play');
    await click(first);
    assert.deepEqual(players(), ['Pause voice message', 'Play voice message']);
    await click(second);
    assert.deepEqual(players(), ['Play voice message', 'Pause voice message']);
    const audio = document.querySelectorAll('audio')[1];
    await render(() => createElement('p', null, 'Another conversation'));
    assert.equal(audio.paused, true);
    assert.equal(button('Pause voice message'), undefined);
  }, { url });
});

test('the waveform slider seeks with the keyboard and says where it is', async () => {
  await reactFixture(async ({ render, update, dom }) => {
    playableMedia(dom);
    const { MessageFiles } = await import('../src/react/results/files.tsx');
    await render(MessageFiles, { files: [voice('seek', { duration_ms: 38_000 })] });
    assert.equal(slider().getAttribute('tabindex'), '0');
    assert.equal(slider().getAttribute('aria-label'), 'Voice message position');
    assert.equal(slider().getAttribute('aria-valuemax'), '38');
    assert.equal(slider().getAttribute('aria-valuetext'), '0:00 of 0:38');
    assert.equal(document.querySelector('.voice-length').textContent, '0:38');
    await update(() => key(dom, 'ArrowRight'));
    assert.equal(slider().getAttribute('aria-valuenow'), '5');
    assert.equal(slider().getAttribute('aria-valuetext'), '0:05 of 0:38');
    assert.equal(document.querySelector('audio').currentTime, 5);
    await update(() => key(dom, 'ArrowUp'));
    await update(() => key(dom, 'ArrowLeft'));
    assert.equal(slider().getAttribute('aria-valuenow'), '5');
    await update(() => key(dom, 'End'));
    assert.equal(slider().getAttribute('aria-valuetext'), '0:38 of 0:38');
    await update(() => key(dom, 'PageUp'));
    assert.equal(slider().getAttribute('aria-valuenow'), '38', 'seeking stops at the end');
    await update(() => key(dom, 'Home'));
    await update(() => key(dom, 'ArrowDown'));
    assert.equal(slider().getAttribute('aria-valuenow'), '0');
    // Playback moves the same position. Without Web Audio here, a plain bar fills instead of the waveform.
    const audio = document.querySelector('audio');
    await update(() => { audio.currentTime = 19; audio.dispatchEvent(new dom.window.Event('timeupdate')); });
    assert.equal(slider().getAttribute('aria-valuetext'), '0:19 of 0:38');
    assert.equal(document.querySelector('.voice-progress > span').style.width, '50%');
  }, { url });
});

test('the waveform decodes once the player is visible; a decoding failure falls back to a progress bar', async () => {
  const fetched = [];
  await reactFixture(async ({ render, update, dom }) => {
    playableMedia(dom);
    let visible;
    dom.window.IntersectionObserver = globalThis.IntersectionObserver = class { constructor(callback) { visible = () => callback([{ isIntersecting: true }]); } observe() {} disconnect() {} };
    let decode = 'works';
    dom.window.OfflineAudioContext = class {
      constructor(channels, length, rate) { assert.deepEqual([channels, length, rate], [1, 1, 16000]); }
      decodeAudioData(bytes, resolve, reject) {
        if (decode === 'works') { const samples = new Float32Array(16000 * 12).map((_, index) => Math.sin(index / 50) * (index / 192000)); resolve({ duration: 12, getChannelData: () => samples }); }
        else reject(new DOMException('Unable to decode audio data', 'EncodingError'));
        return decode === 'works' ? Promise.resolve() : Promise.reject(new DOMException('Unable to decode audio data', 'EncodingError'));
      }
    };
    const { MessageFiles } = await import('../src/react/results/files.tsx');
    // Without a server length (transcription off), the decoded length shows.
    await render(MessageFiles, { files: [voice('decoded')] });
    assert.equal(fetched.length, 0, 'nothing is downloaded before the message is visible');
    assert.equal(document.querySelectorAll('.voice-wave i').length, 40, 'quiet placeholder bars hold the place');
    await update(() => visible());
    assert.equal(fetched.length, 1);
    assert.match(fetched[0], /\/objects\/decoded\?inline=true$/);
    assert.equal(document.querySelector('.voice-length').textContent, '0:12');
    const heights = [...document.querySelectorAll('.voice-wave i')].map(bar => parseInt(bar.style.height, 10));
    assert.equal(heights.length, 40);
    assert.ok(heights.at(-1) > heights[1], 'louder parts draw taller bars');
    const audio = document.querySelector('audio');
    await update(() => { audio.currentTime = 3; audio.dispatchEvent(new dom.window.Event('timeupdate')); });
    assert.equal(document.querySelectorAll('.voice-wave i.is-played').length, 10, 'a quarter of the bars fill at 0:03 of 0:12');

    decode = 'fails';
    await render(MessageFiles, { files: [voice('broken', { duration_ms: 7000 })] });
    await update(() => visible());
    assert.equal(document.querySelector('.voice-wave'), null);
    assert.ok(document.querySelector('.voice-progress'));
    assert.equal(slider().getAttribute('aria-valuetext'), '0:00 of 0:07');
    assert.equal(document.querySelector('.voice-length').textContent, '0:07');

    // A remounted message reuses its waveform without downloading it again.
    const before = fetched.length;
    await render(() => createElement('p', null, 'Elsewhere'));
    await render(MessageFiles, { files: [voice('decoded')] });
    assert.equal(fetched.length, before);
    assert.equal(document.querySelectorAll('.voice-wave i').length, 40);
    delete globalThis.IntersectionObserver;
  }, { url, fetch: async path => { fetched.push(path); return { ok: true, status: 200, arrayBuffer: async () => new ArrayBuffer(8) }; } });
});

test('peaks are the loudest sample of each slice, scaled to the loudest', async () => {
  const { summarizePeaks } = await import('../src/react/results/voice-peaks.ts');
  assert.deepEqual(summarizePeaks(new Float32Array([0.1, -0.5, 0.25, 0.25]), 2), [1, 0.5]);
  assert.deepEqual(summarizePeaks(new Float32Array([0, 0]), 2), [0, 0], 'silence stays flat');
  assert.equal(summarizePeaks(new Float32Array(3), 40).length, 40);
});

test('uploaded audio files keep their name and download beside the player', async () => {
  await reactFixture(async ({ render }) => {
    const { MessageFiles } = await import('../src/react/results/files.tsx');
    await render(MessageFiles, { files: [{ id: 'song', name: 'site-walkthrough.mp3', mime: 'audio/mpeg', size: 2048, checksum: 'song-sum' }] });
    assert.equal(document.querySelector('.voice-play').getAttribute('aria-label'), 'Play site-walkthrough.mp3');
    assert.ok(document.querySelector('[aria-label="Download site-walkthrough.mp3"]'));
  }, { url });
});

test('a voice-only message shows its player without the files-only placeholder; the time shares the transcript line', async () => {
  await reactFixture(async ({ render }) => {
    globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { ChatMessage } = await import('../src/react/chat/message.tsx');
    const directory = { users: [{ id: 'owner', name: 'Priya' }], agents: [], role: 'Admin' };
    const message = { id: 'own', chat: 'room', author: 'owner', kind: 'user', name: 'Priya', content: 'Shared files', state: 'sent', at: 1, attachments: [voice('own-note', { transcript_status: 'ready', transcript: 'Photos follow.' })] };
    await render(ChatMessage, { message, directory });
    assert.equal(document.body.textContent.includes('Shared files'), false);
    assert.ok(document.querySelector('.chat-voice-bubble > .result-files .voice-player'));
    assert.ok(document.querySelector('.chat-voice-bubble > .chat-message-meta'));
    // Typed words still show; so does the placeholder text when nothing is attached.
    await render(ChatMessage, { message: { ...message, content: 'Photos to follow' }, directory });
    assert.match(document.querySelector('.message-markdown').textContent, /Photos to follow/);
    assert.equal(document.querySelector('.chat-voice-bubble'), null);
    await render(ChatMessage, { message: { ...message, attachments: [] }, directory });
    assert.match(document.body.textContent, /Shared files/);
    delete globalThis.IntersectionObserver;
  }, { url, fetch: session });
});
