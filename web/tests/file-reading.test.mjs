import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const doc = (id, mime, reading) => ({ id, name: `${id}.pdf`, mime, size: 4096, checksum: `${id}-hash`, reading });
const label = () => document.querySelector('.chat-file-reading');

test('a shared document says while it is read for agents and when they can’t read it', async () => {
  await reactFixture(async ({ render }) => {
    const { MessageFiles } = await import('../src/react/results/files.tsx');
    await render(MessageFiles, { files: [doc('quote', 'application/pdf', 'pending')] });
    assert.equal(document.querySelector('[role="status"]')?.textContent, 'Reading for agents…');

    await render(MessageFiles, { files: [doc('scan', 'application/pdf', 'unsupported')] });
    assert.equal(label()?.textContent, "Agents can't read this file");
    assert.equal(label()?.getAttribute('role'), null, 'a settled state is not announced');

    await render(MessageFiles, { files: [doc('broken', 'application/octet-stream', 'failed')] });
    assert.match(label()?.textContent ?? '', /^Agents can't read this file$/);
  });
});

test('readable, unread, image and audio files stay quiet', async () => {
  await reactFixture(async ({ render }) => {
    const { MessageFiles } = await import('../src/react/results/files.tsx');
    for (const file of [
      doc('ready', 'application/pdf', 'ready'),
      // Uploaded before agents could read files, or an unsent upload.
      doc('old', 'application/octet-stream', 'not_read'),
      doc('unsent', 'text/plain', undefined),
      // Photos usually carry no text; people can see them anyway.
      doc('photo', 'image/jpeg', 'unsupported'),
      { ...doc('voice', 'audio/webm', undefined), transcript_status: 'not_enabled' },
    ]) {
      await render(MessageFiles, { files: [file] });
      assert.ok(!label(), `${file.id} has no reading label`);
    }
  });
});
