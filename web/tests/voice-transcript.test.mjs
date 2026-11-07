import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const voice = (id, extra) => ({ id, name: `${id}.webm`, mime: 'audio/webm', size: 4096, checksum: `${id}-hash`, ...extra });

test('a voice note folds its transcript away under its player', async () => {
  await reactFixture(async ({ render, button, click }) => {
    const { MessageFiles } = await import('../src/react/results/files.tsx');
    await render(MessageFiles, { files: [
      voice('ready', { transcript_status: 'ready', transcript: ' Book the van for Friday. ' }),
    ] });
    const toggle = button('Transcript');
    assert.equal(toggle.getAttribute('aria-expanded'), 'false');
    const text = document.getElementById(toggle.getAttribute('aria-controls'));
    assert.equal(text.hidden, true);
    await click(toggle);
    assert.equal(toggle.getAttribute('aria-expanded'), 'true');
    assert.equal(text.hidden, false);
    assert.equal(text.textContent, 'Book the van for Friday.');
    await click(toggle);
    assert.equal(text.hidden, true);
    assert.equal(document.querySelectorAll('audio[controls]').length, 1);
  });
});

test('pending, failed and untranscribed voice notes show only their state', async () => {
  await reactFixture(async ({ render, button }) => {
    const { MessageFiles } = await import('../src/react/results/files.tsx');
    await render(MessageFiles, { files: [voice('pending', { transcript_status: 'pending', transcript: null })] });
    assert.equal(document.querySelector('[role="status"]')?.textContent, 'Transcribing…');
    assert.ok(!button('Transcript'));

    await render(MessageFiles, { files: [voice('failed', { transcript_status: 'failed', transcript: null })] });
    assert.match(document.querySelector('.chat-transcript')?.textContent ?? '', /^Transcript unavailable$/);
    assert.ok(!button('Transcript'));

    // Where the deployment doesn't transcribe, and on uploads not yet sent.
    await render(MessageFiles, { files: [voice('off', { transcript_status: 'not_enabled' }), voice('unsent')] });
    assert.ok(!document.querySelector('.chat-transcript'));
    assert.equal(document.querySelectorAll('audio[controls]').length, 2);

    // A silent recording still opens, saying so.
    await render(MessageFiles, { files: [voice('silent', { transcript_status: 'ready', transcript: '' })] });
    assert.ok(button('Transcript'));
    assert.match(document.querySelector('.chat-transcript p')?.textContent ?? '', /No speech was recognised/);

    // Other files never show a transcript line.
    await render(MessageFiles, { files: [{ id: 'doc', name: 'quote.pdf', mime: 'application/octet-stream', size: 10, checksum: 'doc-hash' }] });
    assert.ok(!document.querySelector('.chat-transcript'));
  });
});
