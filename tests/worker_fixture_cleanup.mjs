import { test } from 'node:test';
import assert from 'node:assert/strict';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/polling.mjs';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';

test('fixture cleanup stops all owned processes and saves evidence after one stop times out',
  { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 45000 }, async () => {
    const hooks = [];
    const app = await workerFixture({ after: hook => hooks.push(hook) }, { mode: 'pooled', control: true });
    const errors = [];
    try {
      const blocked = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
      await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
      // Executors handle SIGTERM once registered; cleanup before that is a different case.
      await until(() => Number(app.sql("SELECT count(*) FROM relay_executor_slots WHERE state='ready'")) === 2,
        'Executors did not report ready slots', 30000);
      blocked.child.kill('SIGSTOP');
      for (const hook of hooks) {
        try { await hook(); } catch (error) { errors.push(error); }
      }
      assert.equal(errors.length, 1);
      assert.match(errors[0].message, /Worker fixture cleanup failed/);
      await until(() => app.processes().every(record => record.child.exitCode !== null
        || record.child.signalCode !== null), 'Cleanup retained another owned process');
      const report = JSON.parse(await readFile(join(app.data, 'report.json'), 'utf8'));
      assert.equal(report.cleanup_errors, 1);
      assert.ok(report.processes.every(record => record.code !== null || record.signal !== null));
    } finally {
      for (const record of app.processes()) {
        if (record.child.exitCode === null && record.child.signalCode === null) record.child.kill('SIGKILL');
      }
    }
  });
