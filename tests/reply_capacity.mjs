import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/polling.mjs';
import { exited } from './fixtures/worker_control.mjs';

const options={skip:!process.env.RELAY_TEST_DATABASE_URL,timeout:180000};
const ready=async(app,n)=>until(()=>Number(app.sql("SELECT count(*) FROM relay_executor_slots WHERE state='ready'"))>=n,'Declared ready reserve missing',35000);
function quantiles(values){const sorted=[...values].sort((a,b)=>a-b);return Object.fromEntries([50,95,99].map(p=>['p'+p,sorted[Math.min(sorted.length-1,Math.ceil(sorted.length*p/100)-1)]]));}
async function resources(executors){
 const samples=[];
 for(const executor of executors){
  for(const pid of [executor.child.pid,...await executor.app.gooseChildren(executor)]){
   try{const status=await readFile(`/proc/${pid}/status`,'utf8');const stat=(await readFile(`/proc/${pid}/stat`,'utf8')).split(') ')[1].split(' ');
    samples.push({pid,supervisor:pid===executor.child.pid,rss_bytes:Number(status.match(/^VmRSS:\s+(\d+)/m)?.[1]??0)*1024,cpu_ticks:Number(stat[11])+Number(stat[12])});
   }catch{/* A replaced harness is not a live RSS sample. */}
  }
 }
 return samples;
}

test('named local workload measures warm dispatch and rehearses queued scale-out and active drain',options,async t=>{
 const app=await workerFixture(t,{mode:'pooled',control:true,resumableHold:true,capacity:6,workspaceLimit:6,modelLimit:6});
 app.provider.setStream({chunks:['Measured response 🦀'],holdAfter:9});
 const fleet=[await app.startExecutor(),await app.startExecutor()];fleet.forEach(e=>{e.app=app;});await ready(app,4);
 const start=Date.now(),initial=await resources(fleet);const warm=[];
 for(let sample=0;sample<24;sample++){
  await ready(app,4);
  const chat=(await app.request(app.path+'/chats',app.cookie,{id:randomUUID(),name:'Bounded benchmark',kind:'group',users:[],agents:[app.profile.id]})).value.id;
  const providerIndex=app.provider.requests.length;
  const job=await app.send('A bounded ordinary benchmark request.',chat);await app.done(job);
  const times=JSON.parse(app.sql(`SELECT json_build_object('accepted_ms',j.created_ms,'reserved_ms',a.created_ms,'context_ready_ms',a.context_ready_ms,'prompt_reported_ms',a.prompt_reported_ms,'first_output_ms',a.first_output_ms,'finished_ms',j.finished_ms) FROM relay_chat_jobs j JOIN relay_worker_assignments a ON a.id=j.claim WHERE j.reply='${job.reply}'`));
  const provider=app.provider.timings.filter(v=>v.index===providerIndex);
  const requested=provider.find(v=>v.phase==='request')?.time_ms;
  const token=provider.find(v=>v.phase==='first_token')?.time_ms;
  assert.ok(requested && token && times.first_output_ms && times.finished_ms);
  warm.push({...times,provider_request_ms:requested,provider_first_token_ms:token,prompt_characters:JSON.stringify(app.provider.requests[providerIndex].messages).length});
 }
 const afterWarm=await resources(fleet),warmEnd=Date.now();
 // A long model wait occupies one slot while short chat can use the rest.
 app.provider.setStream({chunks:['LONG_PARTIAL',' finished'],holdAfter:1});app.provider.setOutputHeld(true);
 const long=await app.send('A held long-running conversation');
 await until(async()=>(await app.history()).find(m=>m.id===long.reply)?.progress?.text==='LONG_PARTIAL','Long reply did not start');
 app.provider.setStream({chunks:['SHORT_RESPONSE'],holdAfter:9});
 for(let n=0;n<3;n++){
  const agent={...app.profile,id:randomUUID(),name:'Short '+n};await app.request(app.path+'/agent-profiles',app.cookie,agent);
  const chat=(await app.request(app.path+'/chats',app.cookie,{id:randomUUID(),name:'Short request',kind:'group',users:[],agents:[agent.id]})).value.id;
  await app.done(await app.send('Short chat alongside a long request',chat,app.cookie,agent.id));
 }
 assert.equal(app.sql(`SELECT state FROM relay_chat_jobs WHERE reply='${long.reply}'`),'running');
 app.provider.setOutputHeld(false);await app.done(long);await ready(app,4);
 // Start above ready capacity; only prepared sessions may consume attempts.
 app.provider.setHeld(true);const before=app.provider.requests.length;const burst=[];
 for(let n=0;n<8;n++){
  const agent={...app.profile,id:randomUUID(),name:'Burst '+n};await app.request(app.path+'/agent-profiles',app.cookie,agent);
  const chat=(await app.request(app.path+'/chats',app.cookie,{id:randomUUID(),name:'Burst request',kind:'group',users:[],agents:[agent.id]})).value.id;
  burst.push(await app.send('A bounded burst request',chat,app.cookie,agent.id));
 }
 await until(()=>app.provider.requests.length===before+4,'Initial fleet did not fill four slots');
 assert.equal(burst.filter(j=>app.attempts(j)===0).length,4);
 const scaleBegan=Date.now();await delay(500);const third=await app.startExecutor();
 await until(()=>app.provider.requests.length===before+6,'New prepared capacity did not admit queued work',35000);
 const scaleReady=Date.now();assert.equal(burst.filter(j=>app.attempts(j)===0).length,2);
 third.child.kill('SIGTERM');await until(()=>app.sql("SELECT count(*) FROM relay_executors WHERE state='draining'")==='1','Active executor did not drain');
 app.provider.setHeld(false);for(const job of burst)await app.done(job);
 assert.equal(await exited(third),0);assert.ok(burst.every(j=>app.attempts(j)===1));
 const distributions={};
 for(const [name,key] of Object.entries({dispatch:'reserved_ms',context:'context_ready_ms',harness_handoff_report:'prompt_reported_ms',fixture_model_request:'provider_request_ms',fixture_first_token:'provider_first_token_ms',first_saved_text:'first_output_ms',final_save:'finished_ms'})){
  distributions[name]=quantiles(warm.map(s=>s[key]-s.accepted_ms));
 }
 const hz=Number(execFileSync('getconf',['CLK_TCK'],{encoding:'utf8'}));
 const supervisorCpu=afterWarm.filter(s=>s.supervisor).reduce((sum,s)=>sum+s.cpu_ticks-(initial.find(p=>p.pid===s.pid)?.cpu_ticks??s.cpu_ticks),0)/hz;
 const report={workload:'local-warm-24-fresh-conversations-four-slots',provider:'scripted loopback, real pinned Goose',samples:warm.length,slots:4,executors:2,warm_wall_ms:warmEnd-start,distributions_ms:distributions,proposed_target_results:{dispatch_p95_under_100_ms:distributions.dispatch.p95<100,fixture_model_request_p95_under_250_ms:distributions.fixture_model_request.p95<250},warm_samples:warm,resources:{initial,after_warm:afterWarm,supervisor_cpu_seconds:supervisorCpu,scope:'Live RSS samples and supervisor CPU only; exited harness CPU is excluded.'},burst:{accepted:8,initial_active:4,initial_queued:4,after_growth_active:6,after_growth_queued:2,simulated_detection_delay_ms:500,additional_capacity_observed_ms:scaleReady-scaleBegan,active_drain_finished:true},limits:'Local fixture timings do not establish live-provider first-token latency, cloud readiness, multi-hour soak or production capacity.'};
 await writeFile(app.data+'/capacity-report.json',JSON.stringify(report,null,2));
 console.log('Warm local distributions:',JSON.stringify(distributions));console.log('Proposed targets:',JSON.stringify(report.proposed_target_results));
 for(const executor of fleet)await app.stop(executor);
 app.pass('24 bounded warm samples, short chat beside held work, saturation, delayed executor growth and active drain retain exact durable ownership');
});
