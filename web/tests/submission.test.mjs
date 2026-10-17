import { test } from 'node:test';
import assert from 'node:assert/strict';
import { PendingSubmission } from '../src/submission.ts';

test('a lost acknowledgement keeps the same request identity through retry and reload', () => {
  const values = new Map();
  const storage = { getItem: (k) => values.get(k), setItem: (k, v) => values.set(k, v), removeItem: (k) => values.delete(k) };
  const pending = new PendingSubmission(storage);
  const first = pending.prepare('Check inventory');
  assert.deepEqual(pending.prepare('Check inventory'), first);
  const reloaded = new PendingSubmission(storage);
  assert.equal(reloaded.draft(), 'Check inventory');
  assert.deepEqual(reloaded.prepare('Check inventory'), first);
  reloaded.acknowledge(first.request_id);
  assert.equal(new PendingSubmission(storage).draft(), '');
  assert.notEqual(reloaded.prepare('Check inventory').request_id, first.request_id);
});

test('edited content gets a new identity and storage failure still permits stable retries', () => {
  const storage = { getItem() { throw new Error('blocked'); }, setItem() { throw new Error('blocked'); } };
  const pending = new PendingSubmission(storage);
  const first = pending.prepare('First');
  assert.equal(pending.prepare('First').request_id, first.request_id);
  const edited = pending.prepare('Changed');
  assert.notEqual(edited.request_id, first.request_id);
  pending.acknowledge(first.request_id);
  assert.equal(pending.draft(), 'Changed');
});

test('pending messages remain separate when switching agents', () => {
  const values = new Map();
  const storage = { getItem: k => values.get(k), setItem: (k,v) => values.set(k,v), removeItem: k => values.delete(k) };
  const first = new PendingSubmission(storage);
  const second = new PendingSubmission(storage, 'second-agent');
  const a = first.prepare('Private task A');
  const b = second.prepare('Private task B');
  assert.equal(new PendingSubmission(storage).draft(), a.content);
  assert.equal(new PendingSubmission(storage, 'second-agent').draft(), b.content);
  second.acknowledge(b.request_id);
  assert.equal(new PendingSubmission(storage).draft(), a.content);
});

test('attachment selection is part of the retry identity', () => {
  const values = new Map();
  const storage = { getItem: k => values.get(k), setItem: (k,v) => values.set(k,v) };
  const pending = new PendingSubmission(storage, 'files-agent');
  const first = pending.prepare('Review', ['file-a']);
  assert.deepEqual(new PendingSubmission(storage, 'files-agent').prepare('Review', ['file-a']), first);
  assert.notEqual(pending.prepare('Review', ['file-b']).request_id, first.request_id);
});
