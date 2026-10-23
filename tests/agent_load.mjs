import { test } from 'node:test';
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { pinLoadProcess, sampleResources } from './fixtures/load_resources.mjs';
import { summarize } from './fixtures/load_metrics.mjs';
import { actors, requestCase, ready, send, complete, samples } from './fixtures/load_workload.mjs';

function integer(name, fallback, min, max) {
  const value = Number(process.env[name] ?? fallback);
  assert.ok(Number.isInteger(value) && value >= min && value <= max, `Invalid ${name}`);
  return value;
}
function configuration() {
  const concurrency = (process.env.AYATI_LOAD_CONCURRENCY ?? '1,4,8,16').split(',').map(Number);
  assert.ok(concurrency.length > 0 && concurrency.every(n => [1,4,8,16].includes(n)));
  const contexts = (process.env.AYATI_LOAD_CONTEXTS ?? 'small,large').split(',');
  assert.ok(contexts.every(s => ['small','large'].includes(s)));
  return { concurrency, contexts, samples: integer('AYATI_LOAD_SAMPLES',24,4,40), repeats: integer('AYATI_LOAD_REPEATS',2,1,5), cores: integer('AYATI_LOAD_CORES',4,2,16),
    slots:4, executors:2, model_first_delay_ms:30, lease_seconds:12 };
}
test('controlled matrix retains repeated warm and queued latency with correlated traces', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 900000,
}, async t => {
  const config = configuration(), environment = await pinLoadProcess(t, config.cores);
  const app = await workerFixture(t, { mode:'pooled',control:true,traceStart:true,cpuList:environment.cpu_list,
    capacity:4,workspaceLimit:32,modelLimit:32,leaseSeconds:config.lease_seconds });
  app.provider.setStream({chunks:['CONTROLLED_LOAD_OK'],holdAfter:9,firstDelayMs:config.model_first_delay_ms});
  const fleet = [await app.startExecutor(),await app.startExecutor()]; await ready(app);
  const actorSets = {};
  for (const tier of config.contexts) actorSets[tier] = await actors(app,Math.max(...config.concurrency),tier);
  const report = { version:1,config,environment,binary:process.env.AYATI_TEST_BIN,cases:[],failures:[],
    limits:environment.boundary + ' Local scripted model and asynchronous observers; no cloud/live-provider/production capacity claim.' };
  const save = () => writeFile(app.data+'/load-report.json',JSON.stringify(report,null,2));
  await save();
  try {
    for (let repeat=1;repeat<=config.repeats;repeat++) for (const tier of config.contexts) for (const concurrency of config.concurrency) {
      const input = [];
      for (let i=0;i<config.samples;i++) input.push(await requestCase(app,actorSets[tier][i%concurrency],tier));
      const result = { repeat,tier,concurrency,ready_slots:4,kind:concurrency<=4?'ready_capacity':'overflow',raw:[] };
      report.cases.push(result); await save();
      const stopResources = await sampleResources([process.pid,app.apiProcess().child.pid,...fleet.map(e=>e.child.pid)]);
      const began = Date.now();
      try {
        for (let offset=0;offset<input.length;offset+=concurrency) {
          await ready(app);
          const wave=input.slice(offset,offset+concurrency);
          await Promise.all(wave.map(item=>send(app,item)));
          result.raw.push(...samples(app,wave,await complete(app,wave)));
        }
        result.wall_ms=Date.now()-began;
        result.summary=summarize(result.raw);
      } finally {result.resources=await stopResources();await save();}
      console.log('LOAD_CASE',JSON.stringify({repeat,tier,concurrency,kind:result.kind,...result.summary,resources:result.resources}));
    }
    report.ready_capacity_targets_met=report.cases.filter(c=>c.kind==='ready_capacity').every(c=>Object.values(c.summary.provisional_targets).every(Boolean));
    report.correctness_passed=true;await save();
    console.log('LOAD_ACCEPTANCE',JSON.stringify({correctness:true,ready_capacity_targets_met:report.ready_capacity_targets_met}));
  } catch(error) {report.failures.push(String(error));report.correctness_passed=false;await save();throw error;}
  for (const executor of fleet) await app.stop(executor);
  app.pass('Repeated bounded contexts and concurrent/overflow waves preserve one inference, scoped history and approved knowledge; every target miss is retained');
});
