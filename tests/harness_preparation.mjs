import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdir, mkdtemp, writeFile, readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fixtureProvider } from './fixtures/goose_provider.mjs';
import { gooseAcp } from './fixtures/goose_acp.mjs';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { pinLoadProcess } from './fixtures/load_resources.mjs';
import { until } from './fixtures/polling.mjs';
import { quantiles } from './fixtures/load_metrics.mjs';

const root = resolve(import.meta.dirname, '..');
const prepared = process.env.AYATI_TEST_PREPARED_GOOSE_BIN ?? join(root, '.cache/goose/v1.54.0-ayati.1/goose');
const options = { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000 };

test('one pinned artifact moves token-counter initialization before readiness without inference', options, async t => {
  const environment = await pinLoadProcess(t, 4);
  const data = await mkdtemp(join(root, '.tmp/goose-preparation-'));
  const report = { version: 1, environment, artifact: prepared, samples: [] };
  const provider = await fixtureProvider();
  t.after(async () => {
    provider.close();
    await writeFile(data + '/report.json', JSON.stringify({ ...report, passed: t.passed }, null, 2));
    console.log('Retained preparation evidence:', data);
  });
  // Interleave modes; every trial uses a fresh process/session and the same ELF.
  for (let round = 0; round < 4; round++) for (const warm of round % 2 ? [true, false] : [false, true]) {
    const folder = data + `/trial-${report.samples.length}`;
    await mkdir(folder); await writeFile(folder + '/.env', '');
    const client = await gooseAcp(prepared, folder, provider);
    try {
      await client.request('initialize', { protocolVersion: 1, clientCapabilities: {}, clientInfo: { name: 'ayati-preparation-probe', version: '1' } });
      const before = provider.requests.length, begin = Date.now();
      const meta = { enabledExtensions: [], sessionTitle: 'Empty generic preparation', ...(warm ? { ayatiPrepare: 1 } : {}) };
      const session = await client.request('session/new', { cwd: folder, mcpServers: [], _meta: meta });
      const ready = Date.now();
      assert.equal(provider.requests.length, before, 'Preparation performed inference');
      if (warm) {
        assert.equal(session._meta.ayatiPrepared.version, 1);
        assert.equal(session._meta.ayatiPrepared.tokenCounter, 'o200k_base');
        assert.ok(session._meta.ayatiPrepared.durationMicros >= 0);
      } else assert.equal(session._meta?.ayatiPrepared, undefined);
      const sent = Date.now();
      await client.request('session/prompt', { sessionId: session.sessionId, prompt: [{ type: 'text', text: `PREPARATION_TRIAL_${report.samples.length}: answer briefly` }] });
      assert.equal(provider.requests.length, before + 1);
      assert.equal(client.unexpected, 0, 'Warm-up requested a host capability');
      const request = provider.timings.find(event => event.index === before && event.phase === 'request');
      report.samples.push({ warm, prepare_ms: ready - begin, prompt_to_model_ms: request.time_ms - sent,
        generic_initialization_us: session._meta?.ayatiPrepared?.durationMicros });
    } finally { await client.close(); await writeFile(folder + '/process.log', client.logs); }
  }
  for (const warm of [false, true]) {
    const rows = report.samples.filter(row => row.warm === warm);
    report[warm ? 'prepared' : 'session_only'] = {
      prepare_ms: quantiles(rows.map(row => row.prepare_ms)), prompt_to_model_ms: quantiles(rows.map(row => row.prompt_to_model_ms)),
    };
  }
  console.log('GOOSE_PREPARATION', JSON.stringify({ session_only: report.session_only, prepared: report.prepared }));
});

test('a version-matching process without the preparation acknowledgment never supplies ready slots', options, async t => {
  const environment = await pinLoadProcess(t, 4);
  const app = await workerFixture(t, { mode: 'pooled', control: true, cpuList: environment.cpu_list });
  const wrapper = app.data + '/unprepared-goose';
  const stock = join(root, '.cache/goose/v1.54.0/goose');
  await writeFile(wrapper, `#!/usr/bin/python\nimport os,sys\nif sys.argv[1:]==['--version']:\n print('1.54.0+ayati.1')\nelse:\n os.execv(${JSON.stringify(stock)},[${JSON.stringify(stock)},*sys.argv[1:]])\n`, { mode: 0o755 });
  const job = await app.send('Wait for preparation acknowledgment');
  const executor = await app.startExecutor({ AYATI_PREPARED_GOOSE_BIN: wrapper });
  await until(() => executor.logs.includes('Harness preparation failed'), 'Missing acknowledgment did not fail preparation', 30000);
  assert.equal(await app.sqlAsync("SELECT count(*) FROM relay_executor_slots WHERE state='ready'"), '0');
  assert.equal(app.attempts(job), 0); assert.equal(app.provider.requests.length, 0);
  await app.stop(executor);
  const healthy = await app.startExecutor(); await app.done(job); await app.stop(healthy);
  app.pass('A matching version string is insufficient; absent generic-preparation acknowledgment leaves queued work unclaimed');
});

test('draining while generic initialization is stopped reaps every child without admitting work', options, async t => {
  const environment = await pinLoadProcess(t, 4);
  const app = await workerFixture(t, { mode: 'pooled', control: true, cpuList: environment.cpu_list });
  const job = await app.send('Keep this queued while preparation stops');
  const wrapper = app.data + '/blocked-preparation';
  await writeFile(wrapper, "#!/usr/bin/python\nimport sys,time\nif sys.argv[1:]==['--version']:\n print('1.54.0+ayati.1')\nelse:\n time.sleep(60)\n", { mode: 0o755 });
  const executor = await app.startExecutor({ AYATI_PREPARED_GOOSE_BIN: wrapper });
  const pids = await until(async () => {
    const pids = await app.gooseChildren(executor), blocked = [];
    for (const pid of pids) {
      try { if ((await readFile(`/proc/${pid}/cmdline`, 'utf8')).includes('\0acp\0')) blocked.push(pid); } catch { /* A version-check process may already have exited. */ }
    }
    return blocked.length === 2 && blocked;
  }, 'Preparing children missing', 15000);
  for (const pid of pids) process.kill(pid, 'SIGSTOP');
  await app.stop(executor);
  await app.assertChildrenStopped(pids);
  assert.equal(app.attempts(job), 0); assert.equal(app.provider.requests.length, 0);
  assert.equal(await app.sqlAsync("SELECT count(*) FROM relay_executor_slots WHERE state='ready'"), '0');
  app.pass('Drain cancels unfinished preparation and reaps stopped children; accepted requests stay queued');
});
