import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { randomUUID } from 'node:crypto';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/polling.mjs';
import { pinLoadProcess, sampleResources } from './fixtures/load_resources.mjs';
import { actors, complete, jobs, ready } from './fixtures/load_workload.mjs';
import { quantiles, startPoints, traces } from './fixtures/load_metrics.mjs';
import { calibrationProxy } from './calibration/proxy.mjs';
import { calibrationBrowser } from './calibration/browser.mjs';
import { scriptedUpstream } from './calibration/scripted.mjs';

const configFile = process.env.AYATI_CALIBRATION_CONFIG;
test('bounded provider calibration measures browser-visible text through the exact release image',
  { skip: !configFile, timeout: 1000000 }, async t => {
    const config = JSON.parse(readFileSync(configFile, 'utf8'));
    const { plan } = config;
    const environment = await pinLoadProcess(t, 4);
    const upstream = config.live ? undefined : await scriptedUpstream();
    if (upstream) t.after(() => upstream.close());
    const proxy = await calibrationProxy({ plan, live: config.live, keyFile: config.key_file,
      scriptedUrl: upstream?.url, reportFile: join(config.evidence, 'provider-ledger.json') });
    t.after(() => proxy.close());
    const report = { version: 1, mode: config.live ? 'live' : 'scripted', plan,
      image: JSON.parse(readFileSync(process.env.AYATI_TEST_IMAGE_CONFIG)).image,
      environment, cases: [], passed: false,
      limitations: 'Same-host synthetic conversations. Browser DOM visibility is observed near a rendering frame; this is not cloud latency, a production SLA or a provider invoice.' };
    const save = () => writeFileSync(join(config.evidence, 'calibration-report.json'), JSON.stringify(report, null, 2) + '\n', { mode: 0o600 });
    save();
    let stopResources;
    try {
      const app = await workerFixture(t, { mode: 'pooled', control: true, traceStart: true,
        model: plan.model, provider: proxy, cpuList: environment.cpu_list,
        capacity: 4, workspaceLimit: 4, modelLimit: 4, chatReserve: 1,
        runSeconds: Math.min(plan.timeout_seconds + 30, 180) });
      const fleet = await Promise.all([app.startExecutor(), app.startExecutor()]);
      await ready(app, 4);
      assert.equal(proxy.snapshot().requests.length, 0, 'Readiness performed inference');
      stopResources = await sampleResources([process.pid, app.apiProcess().child.pid, ...fleet.map(worker => worker.child.pid)]);
      const browser = await calibrationBrowser(t, app);
      async function item(tier, instruction) {
        // Direct conversations are intentionally reused for an agent. Allocate
        // a fresh synthetic agent so every measured first turn has its own scope.
        const [profile] = await actors(app, 1, tier);
        const chat = (await app.request(app.path + '/chats', app.cookie, { id: randomUUID(),
          name: 'Synthetic calibration', kind: 'direct', users: [], agents: [profile.id] })).value.id;
        if (tier === 'large') for (let i = 0; i < 4; i++) await app.request(`${app.path}/chats/${chat}/messages`, app.cookie,
          { id: randomUUID(), content: 'Synthetic inventory note. '.repeat(100) });
        const id = randomUUID(), marker = 'CAL_' + id;
        return { id, marker, chat, agent: profile.id, tier, content: `${marker} ${instruction}` };
      }
      async function collect(items, label) {
        const rows = await complete(app, items, (plan.timeout_seconds + 40) * 1000);
        for (const item of items) {
          const row = rows.find(row => row.chat === item.chat);
          assert.equal(row.message_state, 'complete', 'Calibration reply failed');
          assert.equal(row.attempts, 1, 'Healthy calibration unexpectedly retried');
          const calls = proxy.snapshot().requests.filter(row => row.marker === item.marker);
          assert.equal(calls.length, 1, 'Unexpected inference amplification');
          assert.equal(calls[0].state, 'complete', 'Provider stream/usage did not complete');
          const firstVisible = await browser.visible(item, (plan.timeout_seconds + 10) * 1000);
          const { content, progress, ...stored } = row;
          const measured = { ...stored, marker: item.marker, label, tier: item.tier,
            sent_ms: item.sent_ms, ack_ms: item.ack_ms, browser_first_visible_ms: firstVisible,
            driver_send_began_ms: item.driver_send_began_ms,
            provider: calls[0], start_points: startPoints(app).get(row.claim), trace: traces(app).get(row.claim) };
          report.cases.push(measured); save();
          if (report.cases.length === 1) await browser.capture(item, join(config.evidence, 'chat-small.png'));
          await browser.closePage(item);
        }
        await ready(app, 4);
      }
      for (const tier of ['small', 'large']) {
        const request = await item(tier, 'Reply with a short greeting in one sentence.');
        await browser.watch(request); await browser.send(request); await collect([request], 'individual');
      }
      const wave = await Promise.all([0, 1].map(() => item('small', 'Give a short greeting in one sentence.')));
      for (const request of wave) await browser.watch(request);
      await Promise.all(wave.map(request => browser.send(request))); await collect(wave, 'concurrent-two-agents');
      const background = await item('small', 'CAL_LONG Write a synthetic business summary of about 150 words.');
      const foreground = await item('small', 'Give a short greeting in one sentence.');
      await browser.watch(foreground);
      background.sent_ms = Date.now();
      await app.request(`${app.path}/chats/${background.chat}/background-requests`, app.cookie,
        { id: background.id, content: background.content, agent: background.agent });
      background.ack_ms = Date.now();
      await until(() => proxy.snapshot().requests.find(row => row.marker === background.marker)?.first_text_ms,
        'Background inference did not begin', (plan.timeout_seconds + 20) * 1000);
      report.mixed_overlap_at_send = proxy.snapshot().requests.some(row => row.marker === background.marker && !row.finished_ms);
      await browser.send(foreground); await collect([foreground], 'beside-background');
      await complete(app, [background], (plan.timeout_seconds + 40) * 1000);
      const backgroundCall = proxy.snapshot().requests.find(row => row.marker === background.marker);
      assert.equal(backgroundCall.state, 'complete');
      report.background = { marker: background.marker, provider: backgroundCall };
      const mixed = report.cases.find(row => row.label === 'beside-background');
      report.mixed_overlap_at_provider_request = mixed.provider.forwarded_ms < backgroundCall.finished_ms;
      const concurrent = report.cases.filter(row => row.label === 'concurrent-two-agents');
      report.concurrent_model_overlap = Math.max(...concurrent.map(row => row.provider.forwarded_ms))
        < Math.min(...concurrent.map(row => row.provider.finished_ms));
      const cancelled = await item('small', 'CAL_CANCEL Write a synthetic business summary of about 150 words.');
      await browser.watch(cancelled); await browser.send(cancelled);
      await browser.visible(cancelled, (plan.timeout_seconds + 10) * 1000);
      const cancellationRows = await jobs(app, [cancelled]);
      assert.equal(cancellationRows.length, 1);
      const cancelledPids = await app.assignmentProcessIds([cancellationRows[0].claim]);
      const cancelBegan = Date.now();
      await browser.cancel(cancelled);
      await until(async () => (await app.history(cancelled.chat)).some(message => message.kind === 'agent' && message.state === 'cancelled'),
        'Cancellation did not persist', 20000);
      await until(() => proxy.snapshot().active === 0, 'Upstream did not close after Stop', 20000);
      await app.assertChildrenStopped(cancelledPids, 10000);
      const cancelledCalls = proxy.snapshot().requests.filter(row => row.marker === cancelled.marker);
      assert.equal(cancelledCalls.length, 1);
      assert.equal(cancelledCalls[0].state, 'client_cancelled', 'Model finished before cancellation could be exercised');
      report.cancellation = { observed_ms: Date.now(), began_ms: cancelBegan, provider: cancelledCalls[0] };
      await browser.closePage(cancelled);
      const metrics = { browser_click_to_ack: row => row.ack_ms - row.sent_ms,
        browser_click_to_first_visible: row => row.browser_first_visible_ms - row.sent_ms,
        browser_click_to_final_save: row => row.finished_ms - row.sent_ms,
        server_accept_to_dispatch: row => row.reserved_ms - row.accepted_ms,
        server_accept_to_context: row => row.context_ready_ms - row.accepted_ms,
        server_accept_to_proxy_forward: row => row.provider.forwarded_ms - row.accepted_ms,
        proxy_forward_to_first_text: row => row.provider.first_text_ms - row.provider.forwarded_ms,
        provider_text_to_saved_text: row => row.first_output_ms - row.provider.first_text_ms,
        saved_text_to_browser_visible: row => row.browser_first_visible_ms - row.first_output_ms };
      report.distributions_ms = Object.fromEntries(Object.entries(metrics).map(([name, value]) =>
        [name, quantiles(report.cases.map(value))]));
      report.sample_count = report.cases.length;
      report.quantile_warning = 'Five healthy foreground samples are exploratory; inspect individual cases, not a statistically established p95/p99 or SLA.';
      report.ledger = proxy.snapshot();
      assert.equal(report.ledger.requests.length, 7);
      assert.equal(report.ledger.denied.length, 0);
      assert.deepEqual(browser.errors, []);
      assert.ok(report.mixed_overlap_at_send, 'Long work finished before mixed overlap was observed');
      assert.ok(report.mixed_overlap_at_provider_request, 'Background inference ended before the short model request');
      report.passed = true;
      app.pass('Exact-image synthetic calibration records browser-visible text, bounded provider usage, concurrent/mixed work and real Stop');
    } finally {
      if (stopResources) report.resources = await stopResources();
      report.ledger = proxy.snapshot(); save();
      console.log('Calibration report:', join(config.evidence, 'calibration-report.json'));
    }
  });
