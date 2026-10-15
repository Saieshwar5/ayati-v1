import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, readdir, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { Evidence } from '../e2e/evidence.ts';
import EvidenceReporter from '../e2e/reporter.ts';
import { retainCompletedRuns } from '../e2e/run-history.ts';

const schema = await readFile(new URL('../../src/storage/schema.sql', import.meta.url), 'utf8');

test('failed run evidence retains tool results, redacts credentials and excludes reasoning', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ayati-evidence-'));
  const previousKey = process.env.FIREWORKS_API_KEY;
  process.env.FIREWORKS_API_KEY = 'synthetic-test-key-never-real';
  try {
    const db = new DatabaseSync(join(directory, 'ayati.sqlite'));
    db.exec(schema);
    const context = JSON.stringify([{ role: 'assistant', content: 'Visible result', reasoning_content: 'PRIVATE_REASONING' }]);
    db.prepare("INSERT INTO tasks(id,status,context) VALUES('task','failed',?)").run(context);
    db.prepare("INSERT INTO messages(id,task_id,role,content,state) VALUES('reply','task','assistant',?,'failed')")
      .run('Visible error synthetic-test-key-never-real');
    db.prepare("INSERT INTO tool_attempts(id,task_id,call_id,name,arguments,status,result) VALUES('tool','task','call','shell',?,'error',?)")
      .run(JSON.stringify({ command: 'true', password: 'PRIVATE_PASSWORD' }), JSON.stringify({ output: 'Visible tool failure' }));
    db.close();
    const evidence = new Evidence();
    evidence.check('Task outcome', 'completed', 'failed');
    const attached = [];
    await evidence.save({
      title: 'Failure scenario', status: 'failed', retry: 0, outputDir: directory,
      outputPath: (name) => join(directory, name), attach: async (name) => attached.push(name),
    }, { data: directory, model: 'fixture', logs: 'synthetic-test-key-never-real' }, []);
    const text = await readFile(join(directory, 'evidence.json'), 'utf8');
    const result = JSON.parse(text);
    assert.equal(result.status, 'failed');
    assert.equal(result.tools[0].result.output, 'Visible tool failure');
    assert.deepEqual(result.checks[0], { name: 'Task outcome', expected: 'completed', actual: 'failed' });
    assert.equal(result.token_usage, null);
    assert.doesNotMatch(text, /synthetic-test-key-never-real|PRIVATE_PASSWORD|PRIVATE_REASONING|reasoning_content/);
    assert.equal(await readFile(join(directory, 'daemon.log'), 'utf8'), '[REDACTED]');
    assert.deepEqual(attached, ['Ayati run evidence', 'Daemon log']);
  } finally {
    restoreKey(previousKey);
    await rm(directory, { recursive: true, force: true });
  }
});

test('missing database leaves diagnostic evidence and fails rather than silently passing', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ayati-evidence-missing-'));
  try {
    await assert.rejects(new Evidence().save({
      title: 'Missing database', status: 'passed', retry: 0, outputDir: directory,
      outputPath: (name) => join(directory, name), attach: async () => {},
    }, { data: directory, model: 'fixture', logs: 'Startup did not complete' }, []), /Could not export the test database/);
    const result = JSON.parse(await readFile(join(directory, 'evidence.json'), 'utf8'));
    assert.equal(result.status, 'failed');
    assert.ok(result.events.some((event) => event.kind === 'evidence_error'));
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('report keeps the original failure visible when a retry passes', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ayati-report-'));
  const previousDirectory = process.env.AYATI_TEST_RUN_DIR;
  const previousKey = process.env.FIREWORKS_API_KEY;
  process.env.AYATI_TEST_RUN_DIR = directory;
  process.env.FIREWORKS_API_KEY = 'synthetic-test-key-never-real';
  try {
    await writeFile(join(directory, 'manifest.json'), JSON.stringify({ id: 'run-test', mode: 'fixture', model: 'fixture' }));
    const reporter = new EvidenceReporter();
    const scenario = { id: 'case', title: 'Flaky scenario', expectedStatus: 'passed', outcome: () => 'flaky' };
    reporter.onBegin({}, { allTests: () => [scenario] });
    reporter.onTestEnd(scenario, { retry: 0, status: 'failed', duration: 100,
      errors: [{ message: 'Failure synthetic-test-key-never-real' }], attachments: [] });
    reporter.onTestEnd(scenario, { retry: 1, status: 'passed', duration: 50, errors: [], attachments: [] });
    await reporter.onEnd({ status: 'failed', duration: 150 });
    const text = await readFile(join(directory, 'summary.json'), 'utf8');
    const summary = JSON.parse(text);
    assert.equal(summary.status, 'failed');
    assert.equal(summary.flaky, 1);
    assert.equal(summary.passed, 0);
    assert.deepEqual(summary.scenarios[0].attempts.map((attempt) => attempt.status), ['failed', 'passed']);
    assert.doesNotMatch(text, /synthetic-test-key-never-real/);
  } finally {
    if (previousDirectory === undefined) delete process.env.AYATI_TEST_RUN_DIR;
    else process.env.AYATI_TEST_RUN_DIR = previousDirectory;
    restoreKey(previousKey);
    await rm(directory, { recursive: true, force: true });
  }
});

test('retention removes only old completed Ayati runs and preserves active, foreign and linked folders', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'ayati-run-history-'));
  try {
    for (const [id, finished_at] of [['run-old', '2026-10-01T00:00:00Z'], ['run-new', '2026-10-02T00:00:00Z'], ['run-active', undefined]]) {
      await mkdir(join(directory, id));
      await writeFile(join(directory, id, 'manifest.json'), JSON.stringify({ kind: 'ayati-browser-test', id, finished_at }));
    }
    await mkdir(join(directory, 'run-foreign'));
    await writeFile(join(directory, 'run-foreign', 'manifest.json'), JSON.stringify({ finished_at: '2026-01-01T00:00:00Z' }));
    await symlink(join(directory, 'run-new'), join(directory, 'run-link'));
    await retainCompletedRuns(directory, 1);
    assert.deepEqual((await readdir(directory)).sort(), ['run-active', 'run-foreign', 'run-link', 'run-new']);
    await assert.rejects(retainCompletedRuns(directory, 0), /at least one/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

function restoreKey(previous) {
  if (previous === undefined) delete process.env.FIREWORKS_API_KEY;
  else process.env.FIREWORKS_API_KEY = previous;
}
