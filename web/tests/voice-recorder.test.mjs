import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement, useState } from 'react';
import { reactFixture } from './react-fixture.mjs';
import { outboxStorage } from './outbox-storage-fixture.mjs';

// Voice messages in the composer (D245). Never pass DOM nodes to assert: compare booleans.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=chat`;
const json = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });

/** A microphone whose tracks remember being stopped, and a recorder that stops asynchronously. */
function devices({ fail } = {}) {
  const tracks = [], recorders = [];
  const getUserMedia = async () => {
    if (fail) throw new DOMException('refused', fail);
    const track = { stopped: false, stop() { this.stopped = true; } };
    tracks.push(track); return { getTracks: () => [track] };
  };
  class Recorder {
    static isTypeSupported(type) { return Recorder.supported.includes(type); }
    static supported = ['audio/webm;codecs=opus'];
    state = 'inactive';
    constructor(stream, options) { this.mimeType = options?.mimeType ?? ''; recorders.push(this); }
    start() { this.state = 'recording'; }
    stop() {
      if (this.state === 'inactive') return;
      this.state = 'inactive';
      queueMicrotask(() => { this.ondataavailable?.({ data: new Blob(['voice'], { type: this.mimeType }) }); this.onstop?.(); });
    }
  }
  return { tracks, recorders, getUserMedia, Recorder };
}

async function withDevices(media, run) {
  const navigatorDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'navigator'), recorder = globalThis.MediaRecorder;
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { userAgent: 'node', mediaDevices: media ? { getUserMedia: media.getUserMedia } : undefined } });
  if (media) globalThis.MediaRecorder = media.Recorder; else delete globalThis.MediaRecorder;
  try { await run(); }
  finally {
    if (navigatorDescriptor) Object.defineProperty(globalThis, 'navigator', navigatorDescriptor); else delete globalThis.navigator;
    globalThis.MediaRecorder = recorder;
  }
}

/** The shared composer with a draft and a send that records what it was given. */
async function composer(fixture, { text = '', accept = () => true } = {}) {
  const { ChatComposer } = await import('../src/react/chat/attachments.tsx');
  const sends = [];
  function Harness() {
    const [value, change] = useState(text), [files, setFiles] = useState([]);
    return createElement(ChatComposer, { chat: 'room', value, change, files, setFiles, sendDisabled: !value.trim() && !files.length,
      send: async voice => { sends.push({ text: value, voice: voice?.id }); return accept(); } });
  }
  await fixture.render(Harness);
  return sends;
}

const uploads = [];
function storage(answers = []) {
  uploads.length = 0;
  return async (path, options = {}) => {
    if (!String(path).includes('/objects')) return json({});
    uploads.push({ path, name: options.body?.name, type: options.body?.type });
    const answer = answers.shift() ?? 'ok';
    return answer === 'ok' ? json({ id: `voice-${uploads.length}`, name: options.body.name, mime: options.body.type, size: options.body.size, checksum: 'sum' }) : json({ error: answer }, 503);
  };
}
const key = (dom, target, key, init = {}) => {
  const event = new dom.window.KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(event); return event;
};
const row = () => document.querySelector('.voice-row');
const textarea = () => document.querySelector('textarea');

test('the mic records; Send stops, uploads and sends once with the typed text, then typing resumes', async () => {
  const media = devices();
  await withDevices(media, () => reactFixture(async fixture => {
    const { button, click, update, dom } = fixture;
    const leaving = () => { const event = new dom.window.Event('beforeunload', { cancelable: true }); dom.window.dispatchEvent(event); return event.defaultPrevented; };
    const sends = await composer(fixture, { text: 'Kapoor delivery' });
    await click(button('Record voice message'));
    assert.equal(leaving(), true, 'closing the page while recording asks first');
    assert.ok(button('Cancel recording'), 'the mic becomes the cancel button');
    assert.equal(button('Cancel recording').title, 'Cancel recording');
    assert.equal(textarea().hidden, true);
    assert.equal(document.querySelector('[role=timer]').textContent, '0:00');
    assert.match(row().textContent, /Enter sends · Esc cancels/);
    assert.ok(document.activeElement === row(), 'Enter and Escape reach the recording');
    assert.equal(uploads.length + sends.length, 0, 'nothing is sent while recording');
    const send = button('Send message');
    assert.equal(send.disabled, false, 'Send works with an empty or typed draft');
    await update(() => { send.click(); send.click(); });
    assert.equal(uploads.length, 1);
    assert.deepEqual(sends, [{ text: 'Kapoor delivery', voice: 'voice-1' }]);
    assert.equal(media.recorders[0].state, 'inactive');
    assert.ok(media.tracks.every(track => track.stopped));
    assert.equal(row(), null);
    assert.equal(textarea().hidden, false);
    assert.ok(document.activeElement === textarea(), 'typing continues in the text field');
    assert.ok(button('Record voice message'));
    assert.equal(leaving(), false);
  }, { url, fetch: storage() }));
});

test('Enter sends the recording once, but not while an input method is composing', async () => {
  const media = devices();
  await withDevices(media, () => reactFixture(async fixture => {
    const { button, click, update, dom } = fixture;
    const sends = await composer(fixture);
    await click(button('Record voice message'));
    await update(() => { key(dom, row(), 'Enter', { isComposing: true }); });
    assert.equal(sends.length, 0);
    let first, second;
    await update(() => { first = key(dom, row(), 'Enter'); second = key(dom, row(), 'Enter'); });
    assert.ok(first.defaultPrevented && second.defaultPrevented);
    assert.equal(uploads.length, 1);
    assert.deepEqual(sends, [{ text: '', voice: 'voice-1' }]);
  }, { url, fetch: storage() }));
});

test('the mic again or Escape cancels: tracks stop and nothing is uploaded or kept', async () => {
  const media = devices();
  await withDevices(media, () => reactFixture(async fixture => {
    const { button, click, update, dom } = fixture;
    const sends = await composer(fixture, { text: 'kept draft' });
    await click(button('Record voice message'));
    await click(button('Cancel recording'));
    assert.equal(row(), null);
    assert.ok(button('Record voice message'));
    assert.ok(media.tracks[0].stopped);
    await click(button('Record voice message'));
    let escape;
    await update(() => { escape = key(dom, row(), 'Escape'); });
    assert.ok(escape.defaultPrevented, 'a thread drawer around the composer stays open');
    assert.equal(row(), null);
    assert.ok(media.tracks[1].stopped);
    assert.equal(textarea().value, 'kept draft');
    assert.ok(document.activeElement === textarea());
    assert.equal(uploads.length + sends.length, 0);
    assert.equal(document.querySelector('.voice-draft'), null);
  }, { url, fetch: storage() }));
});

test('a microphone answer after Cancel is released without recording', async () => {
  let resolve;
  const media = devices();
  media.getUserMedia = () => new Promise(done => { resolve = done; });
  await withDevices(media, () => reactFixture(async fixture => {
    const { button, click, update } = fixture;
    await composer(fixture);
    await click(button('Record voice message'));
    assert.match(row().textContent, /Waiting for the microphone/);
    assert.equal(button('Send message').disabled, true);
    await click(button('Cancel recording'));
    let stopped = false;
    await update(() => resolve({ getTracks: () => [{ stop: () => { stopped = true; } }] }));
    assert.equal(stopped, true);
    assert.equal(media.recorders.length, 0);
    assert.ok(button('Record voice message'));
  }, { url, fetch: storage() }));
});

test('a failed upload keeps "not sent" with Retry and Delete; Retry sends once and a refused send re-sends the same upload', async () => {
  const media = devices();
  let accepted = true;
  await withDevices(media, () => reactFixture(async fixture => {
    const { button, click } = fixture;
    const sends = await composer(fixture, { text: 'see audio', accept: () => accepted });
    await click(button('Record voice message'));
    await click(button('Send message'));
    const failed = document.querySelector('.voice-draft.is-failed');
    assert.match(failed.textContent, /Voice message 0:00 not sent/);
    assert.match(failed.textContent, /Storage unavailable/);
    assert.match(document.querySelector('.voice-draft [role=alert]').textContent, /not sent/);
    assert.equal(sends.length, 0);
    assert.equal(button('Record voice message'), undefined, 'one recording at a time');
    assert.equal(textarea().hidden, false, 'the draft can still be edited');
    await click(button('Retry'));
    assert.equal(uploads.length, 2);
    assert.deepEqual(sends, [{ text: 'see audio', voice: 'voice-2' }]);
    assert.equal(document.querySelector('.voice-draft'), null);

    // The conversation refuses the send (it shows why): Retry keeps the uploaded file.
    accepted = false;
    await click(button('Record voice message'));
    await click(button('Send message'));
    assert.match(document.querySelector('.voice-draft').textContent, /not sent/);
    accepted = true;
    await click(button('Send message'));
    assert.equal(uploads.length, 3, 'the recording is not uploaded twice');
    assert.deepEqual(sends.slice(1), [{ text: 'see audio', voice: 'voice-3' }, { text: 'see audio', voice: 'voice-3' }]);

    await click(button('Record voice message'));
    accepted = false; await click(button('Send message'));
    await click(button('Delete'));
    assert.equal(document.querySelector('.voice-draft'), null);
    assert.ok(button('Record voice message'));
  }, { url, fetch: storage(['Storage unavailable']) }));
});

test('at 5 minutes recording stops and waits for Send; the last minute shows the time left', async () => {
  const media = devices(), realNow = Date.now;
  let offset = 0, ticks = [];
  Date.now = () => realNow() + offset;
  try {
    await withDevices(media, () => reactFixture(async fixture => {
      const { button, click, update, dom } = fixture;
      dom.window.setInterval = callback => ticks.push(callback);
      dom.window.clearInterval = () => {};
      const sends = await composer(fixture);
      await click(button('Record voice message'));
      offset = 61_000; await update(() => ticks.at(-1)());
      assert.equal(document.querySelector('[role=timer]').textContent, '1:01');
      assert.equal(document.querySelector('.voice-left'), null);
      offset = 241_000; await update(() => ticks.at(-1)());
      assert.equal(document.querySelector('.voice-left').textContent, '0:59 left');
      assert.equal(row().querySelector('.visually-hidden').textContent, '1 minute left');
      offset = 291_000; await update(() => ticks.at(-1)());
      assert.equal(row().querySelector('.visually-hidden').textContent, '10 seconds left');
      offset = 300_400; await update(() => ticks.at(-1)());
      assert.equal(media.recorders[0].state, 'inactive');
      assert.ok(media.tracks[0].stopped);
      const kept = document.querySelector('.voice-draft');
      assert.match(kept.textContent, /Voice message 5:00 ready to send/);
      assert.match(kept.textContent, /Recording stops at 5 minutes/);
      assert.equal(uploads.length + sends.length, 0, 'the limit never sends by itself');
      // Escape in the text field no longer discards it; only Delete or Send.
      await update(() => { key(dom, textarea(), 'Escape'); });
      assert.ok(document.querySelector('.voice-draft'));
      await update(() => { key(dom, textarea(), 'Enter'); });
      assert.deepEqual(sends, [{ text: '', voice: 'voice-1' }]);
    }, { url, fetch: storage() }));
  } finally { Date.now = realNow; }
});

test('recording stops with a visible message near the 10 MiB upload limit', async () => {
  const media = devices();
  await withDevices(media, () => reactFixture(async fixture => {
    const { button, click, update } = fixture;
    await composer(fixture);
    await click(button('Record voice message'));
    await update(() => media.recorders[0].ondataavailable({ data: new Blob([new Uint8Array(9.5 * 1024 * 1024)]) }));
    assert.match(document.querySelector('.voice-draft').textContent, /Recording stops at 10 MiB/);
    assert.equal(uploads.length, 0);
    await click(button('Delete'));
    assert.equal(document.querySelector('.voice-draft'), null);
  }, { url, fetch: storage() }));
});

test('microphone problems each explain themselves; an unsupported browser still shows the mic', async () => {
  for (const [fail, wording] of [['NotAllowedError', /Microphone access is blocked\. Allow the microphone for this site/], ['NotFoundError', /No microphone was found/], ['NotReadableError', /Another app may be using it/]]) {
    await withDevices(devices({ fail }), () => reactFixture(async fixture => {
      await composer(fixture);
      await fixture.click(fixture.button('Record voice message'));
      assert.match(document.querySelector('[role=alert]').textContent, wording);
      assert.equal(row(), null);
      assert.ok(fixture.button('Record voice message'));
    }, { url, fetch: storage() }));
  }
  const media = devices();
  let asked = 0; const ask = media.getUserMedia; media.getUserMedia = (...args) => { asked++; return ask(...args); };
  await withDevices(media, () => reactFixture(async fixture => {
    fixture.dom.window.isSecureContext = false;
    await composer(fixture);
    await fixture.click(fixture.button('Record voice message'));
    assert.match(document.querySelector('[role=alert]').textContent, /need a secure connection/);
    assert.equal(asked, 0);
  }, { url, fetch: storage() }));
  await withDevices(undefined, () => reactFixture(async fixture => {
    await composer(fixture);
    await fixture.click(fixture.button('Record voice message'));
    assert.match(document.querySelector('[role=alert]').textContent, /can’t use a microphone/);
  }, { url, fetch: storage() }));
  const noRecorder = devices();
  await withDevices(noRecorder, () => reactFixture(async fixture => {
    delete globalThis.MediaRecorder;
    await composer(fixture);
    await fixture.click(fixture.button('Record voice message'));
    assert.match(document.querySelector('[role=alert]').textContent, /can’t record voice messages/);
    assert.equal(noRecorder.tracks.length, 0);
  }, { url, fetch: storage() }));
});

test('voice notes ask for a widely playable format and are named by the recorded type', async () => {
  const media = devices();
  await withDevices(media, () => reactFixture(async fixture => {
    await composer(fixture);
    // Chrome and current Firefox, older Firefox (Ogg only), Safari (MP4 only), then no match.
    for (const types of [['audio/webm;codecs=opus', 'audio/ogg;codecs=opus', 'audio/mp4'], ['audio/ogg;codecs=opus'], ['audio/mp4'], []]) {
      media.Recorder.supported = types;
      await fixture.click(fixture.button('Record voice message'));
      await fixture.click(fixture.button('Send message'));
    }
  }, { url, fetch: storage() }));
  assert.deepEqual(media.recorders.map(recorder => recorder.mimeType), ['audio/webm;codecs=opus', 'audio/ogg;codecs=opus', 'audio/mp4', '']);
  assert.deepEqual(uploads.map(file => `${file.name} ${file.type}`), ['voice-message.webm audio/webm;codecs=opus', 'voice-message.ogg audio/ogg;codecs=opus', 'voice-message.m4a audio/mp4', 'voice-message.webm audio/webm']);
});

test('leaving while recording stops the tracks, the level meter and its audio graph', async () => {
  const media = devices(), frames = [], cancelled = [], contexts = [];
  const created = [], revoked = [], original = { create: URL.createObjectURL, revoke: URL.revokeObjectURL };
  URL.createObjectURL = blob => { const id = `blob:${created.length}`; created.push(id); return id; };
  URL.revokeObjectURL = id => revoked.push(id);
  try {
    await withDevices(media, () => reactFixture(async fixture => {
      const node = () => ({ connect() {}, disconnect() { this.disconnected = true; } });
      fixture.dom.window.AudioContext = class {
        constructor() { contexts.push(this); this.destination = {}; }
        createMediaStreamSource() { return node(); }
        createAnalyser() { return { ...node(), fftSize: 0, getByteTimeDomainData(samples) { samples.fill(160); } }; }
        createGain() { return { ...node(), gain: { value: 1 } }; }
        resume() { return Promise.resolve(); }
        close() { this.closed = true; return Promise.resolve(); }
      };
      fixture.dom.window.requestAnimationFrame = callback => { frames.push(callback); return frames.length; };
      fixture.dom.window.cancelAnimationFrame = id => cancelled.push(id);
      fixture.dom.window.HTMLCanvasElement.prototype.getContext = () => null;
      await composer(fixture);
      await fixture.click(fixture.button('Record voice message'));
      assert.equal(contexts.length, 1);
      await fixture.update(() => frames.at(-1)(1000));
      assert.equal(frames.length, 2, 'the meter keeps drawing while recording');
      await fixture.render(() => createElement('p', null, 'Another conversation'));
      assert.ok(media.tracks[0].stopped);
      assert.equal(media.recorders[0].state, 'inactive');
      assert.deepEqual(cancelled, [2]);
      assert.equal(contexts[0].closed, true);
    }, { url, fetch: storage() }));
  } finally { URL.createObjectURL = original.create; URL.revokeObjectURL = original.revoke; }
  assert.deepEqual(revoked.sort(), created.sort(), 'no object URL outlives the composer');
});

test('in a conversation, Send puts one message with the recording and the draft in the outbox', async () => {
  const previous = globalThis.indexedDB; globalThis.indexedDB = outboxStorage();
  const posts = [], store = storage();
  const media = devices();
  try {
    await withDevices(media, () => reactFixture(async ({ render, button, click, change, update }) => {
      const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
      const box = await import('../src/react/chat/outbox.ts');
      try {
        const { ConversationPane } = await import('../src/react/chat/conversation.tsx');
        await render(ConversationPane, { chat: { id: 'room', name: 'orders', kind: 'channel', users: ['owner'], agents: [] }, directory: { role: 'Admin', users: [{ id: 'owner', name: 'Priya' }], agents: [] }, back: () => {} });
        await change(textarea(), 'Cushions are fine');
        await click(button('Record voice message'));
        const send = button('Send message');
        await update(() => { send.click(); send.click(); });
        for (let i = 0; i < 20 && !posts.length; i++) await update();
        assert.equal(uploads.length, 1);
        assert.equal(posts.length, 1);
        assert.equal(posts[0].content, 'Cushions are fine');
        assert.deepEqual(posts[0].attachments, ['voice-1']);
        assert.equal(textarea().value, '', 'the draft is cleared once the message is in the outbox');
      } finally { await update(() => box.stopOutbox()); }
    }, { url, fetch: async (path, options = {}) => {
      const address = new URL(path, 'http://localhost').pathname;
      if (address.endsWith('/session')) return json({ user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] });
      if (address.endsWith('/objects')) return store(path, options);
      if (options.method === 'POST' && address.endsWith('/chats/room/messages')) { posts.push(JSON.parse(options.body)); return json({ ok: true }); }
      return json({ messages: [], agents: [] });
    } }));
  } finally { globalThis.indexedDB = previous; }
});
