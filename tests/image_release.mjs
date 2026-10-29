import { test } from 'node:test';
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { pinLoadProcess, sampleResources } from './fixtures/load_resources.mjs';
import { ready } from './fixtures/load_workload.mjs';
import { until } from './fixtures/polling.mjs';

const options={skip:!process.env.AYATI_TEST_IMAGE_CONFIG,timeout:180000};
async function fixture(t) {
  const environment=await pinLoadProcess(t,4);
  const app=await workerFixture(t,{mode:'pooled',control:true,traceStart:true,cpuList:environment.cpu_list});
  assert.ok(app.containers);
  return {app,environment};
}
function names(value) { return value.split('\0').filter(Boolean).map(entry=>entry.split('=',1)[0]).sort(); }

test('the built image runs restricted roles with bundled assets, pinned runtimes and actual limits',options,async t=>{
  const {app,environment}=await fixture(t);
  const worker=await app.startExecutor();await ready(app,2);
  assert.equal(app.provider.requests.length,0,'Image readiness performed inference');
  const stopResources=await sampleResources([process.pid,app.apiProcess().child.pid,worker.child.pid]);
  const report={version:1,environment,image:app.containers.config.image,roles:[]};
  for (const record of [app.apiProcess(),worker]) {
    const info=app.containers.inspect(record);
    assert.equal(info.Config.User,'10001:10001');assert.equal(info.HostConfig.Privileged,false);
    assert.equal(info.HostConfig.ReadonlyRootfs,true);assert.equal(info.HostConfig.PidMode,'');
    assert.ok(info.HostConfig.CapDrop.includes('ALL'));
    assert.ok(info.HostConfig.SecurityOpt.includes('no-new-privileges'));
    for (const mount of info.Mounts.filter(m=>m.Type==='bind')) assert.ok(mount.Source.startsWith(app.data+'/'),'Project/owner files entered a runtime container');
    report.roles.push({role:record.role,id:info.Id,user:info.Config.User,pid_namespace:'private',root_read_only:true});
  }
  const hashes=app.containers.exec(worker,['sha256sum','/opt/ayati/ayati','/opt/ayati/goose','/opt/ayati/prepared-goose/goose']);
  const manifest=JSON.parse(app.containers.exec(worker,['cat','/opt/ayati/prepared-goose/manifest.json']));
  assert.equal(manifest.source_commit,'cd643d2126fffab4c6a6186905211ba868d6a657');
  assert.equal(manifest.version,'1.54.0+ayati.1');assert.ok(hashes.includes(manifest.binary_sha256));
  assert.equal(app.containers.exec(worker,['/opt/ayati/goose','--version']).trim(),'1.54.0');
  assert.equal(app.containers.exec(worker,['/opt/ayati/prepared-goose/goose','--version']).trim(),'1.54.0+ayati.1');
  const supervisor=names(app.containers.exec(worker,['cat','/proc/1/environ']));
  assert.ok(!supervisor.some(key=>/^(RELAY_.*DATABASE_URL|WORK_?OS_API_KEY)/.test(key)));
  const children=[...new Set(app.containers.exec(worker,['/bin/sh','-c','for file in /proc/1/task/*/children; do cat "$file"; printf " "; done']).trim().split(/\s+/).filter(Boolean))];
  assert.equal(children.length,2);
  report.goose_environment_names=[];
  for (const pid of children) {
    assert.match(pid,/^\d+$/);
    const keys=names(app.containers.exec(worker,['cat',`/proc/${pid}/environ`]));
    assert.ok(!keys.some(key=>/^(RELAY_|WORK_?OS_|AYATI_WORKER_CONTROL)/.test(key)),'Goose inherited controller or bootstrap configuration');
    report.goose_environment_names.push(keys);
  }
  const response=await fetch(app.base+'/',{headers:{Connection:'close'}});assert.equal(response.status,200);
  const html=await response.text(),asset=html.match(/src="([^"]+\.js)"/)?.[1];assert.ok(asset,'Bundled frontend entry missing');
  const script=await fetch(new URL(asset,app.base),{headers:{Connection:'close'}});assert.equal(script.status,200);assert.ok((await script.text()).length>100);
  const job=await app.send('Reply from the verified container image');await app.done(job);
  assert.equal(app.provider.requests.length,1);assert.equal(app.attempts(job),1);
  report.hashes=hashes.trim().split('\n');report.prepared_manifest=manifest;report.resources=await stopResources();
  await writeFile(app.data+'/image-report.json',JSON.stringify(report,null,2));
  await app.stop(worker);
  app.pass('Exact image serves built assets, prepares without inference, runs real Goose once under verified container limits, and keeps core/bootstrap secrets out of its harness environment');
});

test('a bad packaged harness cannot supply readiness or consume an attempt',options,async t=>{
  const {app}=await fixture(t);
  const job=await app.send('Wait for a healthy container harness');
  const broken=await app.startExecutor({AYATI_PREPARED_GOOSE_BIN:'/missing/prepared-goose'});
  await until(()=>broken.logs.includes('Harness preparation failed'),'Image did not reject missing harness');
  assert.equal(await app.sqlAsync("SELECT count(*) FROM relay_executor_slots WHERE state='ready'"),'0');
  assert.equal(app.attempts(job),0);assert.equal(app.provider.requests.length,0);
  await app.stop(broken);
  const healthy=await app.startExecutor();await app.done(job);await app.stop(healthy);
  assert.equal(app.attempts(job),1);
  app.pass('Unprepared container slots remain inadmissible; a healthy replacement completes the same queued request once');
});

test('the packaged entrypoint and executor reject a core database credential',options,async t=>{
  const {app}=await fixture(t);
  const result=app.containers.refuseCoreSecret();
  assert.equal(result.code,1);
  assert.ok(result.logs.includes('Executors must not receive RELAY_DATABASE_URL'));
  assert.equal(app.provider.requests.length,0);
  app.pass('An offline image refuses core DB configuration before executor registration or inference');
});
