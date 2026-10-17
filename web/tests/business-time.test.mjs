import { test } from 'node:test';
import assert from 'node:assert/strict';
import { localMinute, matchingInstants } from '../src/business-time.ts';
test('business timezone converts independently of the device and handles half-hour offsets', () => {
  const instant = Date.parse('2026-10-17T03:30:00Z');
  assert.equal(localMinute(instant, 'Asia/Kolkata'), '2026-10-17T09:00');
  assert.deepEqual(matchingInstants('2026-10-17T09:00', 'Asia/Kolkata'), [instant]);
  assert.deepEqual(matchingInstants('2026-02-30T09:00', 'UTC'), []);
});
test('DST gaps and folds are explicit instead of silently shifting scheduled work', () => {
  assert.deepEqual(matchingInstants('2026-03-08T02:30', 'America/New_York'), []);
  assert.deepEqual(matchingInstants('2026-11-01T01:30', 'America/New_York'), [Date.parse('2026-11-01T05:30Z'), Date.parse('2026-11-01T06:30Z')]);
  assert.deepEqual(matchingInstants('2026-10-04T02:15', 'Australia/Lord_Howe'), []);
});
