import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { readFile, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { root, until } from './polling.mjs';

// Only used after the owning test stops its source API and executor.
export async function restoredWorkspace(t, app, profiles) {
  const output = join(app.data, 'restore');
  const schema = app.sql('SELECT current_schema()');
  const result = execFileSync('python3', [join(root, 'tools/workspace_restore.py'),
    '--schema', schema, '--files-root', join(app.data, 'api/workspaces'),
    '--output', output, '--source-quiesced', '--keep-target'], {
    env: { ...process.env }, encoding: 'utf8', timeout: 120000,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  assert.equal(JSON.parse(result).passed, true);
  const report = JSON.parse(await readFile(join(output, 'verification.json'), 'utf8'));
  assert.match(report.target_database, /^relay_simulation_restore_[a-f0-9]{32}$/);
  const admin = new URL(process.env.RELAY_TEST_DATABASE_URL);
  const env = {
    PATH: process.env.PATH, LANG: 'C.UTF-8',
    PGDATABASE: report.target_database,
    PGHOST: admin.searchParams.get('host') ?? admin.hostname,
    PGPORT: admin.searchParams.get('port') ?? (admin.port || '5432'),
    PGUSER: decodeURIComponent(admin.username), PGPASSWORD: decodeURIComponent(admin.password),
    PGOPTIONS: `-c search_path=${schema}`, PGPASSFILE: '/dev/null',
  };
  let controller, executor;
  t.after(async () => {
    try {
      await app.stop(executor);
      await app.stop(controller);
    } finally {
      await writeFile(join(output, 'controller.log'), controller?.logs ?? '', { mode: 0o600 });
      execFileSync('dropdb', ['--maintenance-db=' + decodeURIComponent(admin.pathname.slice(1)),
        report.target_database], { env, stdio: 'pipe', timeout: 30000 });
      await writeFile(join(output, 'application-verification.json'), JSON.stringify({
        passed: t.passed, target_cleaned_up: true,
      }, null, 2), { mode: 0o600 });
    }
  });
  const data = dirname(report.target_files);
  // Synthetic test metadata is supplied separately; the backup never copies credentials.
  await writeFile(join(data, '.env'), '', { mode: 0o600 });
  await writeFile(join(data, 'SIMULATION'), '/' + report.target_database, { mode: 0o600 });
  await writeFile(join(data, 'company.json'), await readFile(join(app.data, 'api/company.json')), { mode: 0o600 });
  const runtime = new URL(admin);
  runtime.pathname = '/' + report.target_database;
  runtime.username = 'relay_app';
  runtime.searchParams.set('options', `-c search_path=${schema}`);
  const child = spawn(join(root, 'target/debug/ayati'), ['simulate', data], {
    cwd: app.data, stdio: ['ignore', 'pipe', 'pipe'], env: {
      PATH: process.env.PATH, LANG: 'C.UTF-8', RUST_LOG: 'ayati=info',
      RELAY_SIMULATION_DATABASE_URL: runtime.href, AYATI_PORT: new URL(app.base).port,
      AYATI_CHAT_EXECUTION: 'pooled',
      AYATI_MODEL: 'fixture', AYATI_GOOSE_PROVIDER: 'openai',
      AYATI_GOOSE_BIN: join(root, '.cache/goose/v1.54.0/goose'),
      AYATI_PREPARED_GOOSE_BIN: join(root, '.cache/goose/v1.54.0-ayati.1/goose'),
      AYATI_WORKER_CONTROL_URL: app.control.url, AYATI_WORKER_CONTROL_KEY_FILE: app.control.keyFile,
      AYATI_ASSIGNMENT_LEASE_SECONDS: '15', AYATI_ASSIGNMENT_CAPACITY: '1',
      AYATI_EXECUTOR_WORKSPACE_LIMIT: '1', AYATI_EXECUTOR_MODEL_LIMIT: '1',
      AYATI_EXECUTOR_CHAT_RESERVE: '0', AYATI_MCP_SERVERS_FILE: profiles,
    },
  });
  controller = { child, logs: '' };
  for (const stream of [child.stdout, child.stderr]) {
    stream.on('data', bytes => { controller.logs = (controller.logs + bytes).slice(-65536); });
  }
  await until(async () => {
    assert.equal(child.exitCode, null, controller.logs);
    try { return (await fetch(app.base + '/api/identity/simulation')).ok; }
    catch { return false; }
  }, 'Restored controller did not start');
  return {
    output, report,
    sql(query) {
      try {
        return execFileSync('psql', ['-X', '-At', '-v', 'ON_ERROR_STOP=1'], {
          env, input: query, encoding: 'utf8', stdio: 'pipe', timeout: 30000,
        }).trim();
      } catch { throw new Error('Restored fixture SQL failed'); }
    },
    async startExecutor() {
      executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
      return executor;
    },
  };
}
