// Persistent API rehearsal: no inference, email, destructive cleanup or hidden DB writes.
import assert from 'node:assert/strict';
import { createReadStream } from 'node:fs';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile, readdir } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { clients, randomUUID, distribution, until } from './client.mjs';
import { prepare } from './company.mjs';
import { scenarios } from './scenarios.mjs';
const base=process.argv[2]??'http://127.0.0.1:8876';
if(new URL(base).hostname!=='127.0.0.1')throw new Error('Only loopback simulation instances are supported');
const run=`${new Date().toISOString().replaceAll(':','-')}-${randomUUID().slice(0,8)}`;
const directory=resolve(process.argv[3]??'.ayati-local/company-simulation/runs',run);
await mkdir(directory,{recursive:true,mode:0o700});
const samples=[],checks=[],agentTimes=[];
const {request}=clients(base,samples);
const roster=await request(0,'/api/identity/simulation');
const runNumber=(await readdir(resolve(directory,".."),{withFileTypes:true})).filter(entry=>entry.isDirectory()).length;
const c=await prepare(request,roster,`Rehearsal ${runNumber}`);
const binaryHash=createHash('sha256');for await(const chunk of createReadStream('target/debug/ayati'))binaryHash.update(chunk);
await writeFile(join(directory,'manifest.json'),JSON.stringify({run,base,binary_sha256:binaryHash.digest('hex'),source_head:execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim(),scenario_version:1,...c},null,2));
execFileSync("python",["tools/company-simulation/make-fixture.py",join(directory,"delivery-photo.png")]);
const fixtures=[
 {name:'customer-order.txt',mime:'text/plain',bytes:Buffer.from('Northstar Studio\n20 office desks\nDelivery requested Friday\nPrice must be approved by Finance before confirmation.\n')},
 {name:'stock-count.csv',mime:'text/csv',bytes:Buffer.from('item,ordered,available,shortage\noffice-desk,20,12,8\n')},
 {name:'delivery-photo.png',mime:'image/png',bytes:await readFile(join(directory,'delivery-photo.png'))},
];
// A real PCM audio file exercises binary transport/playback. It is labelled as a tone,
// not speech transcription. A browser test separately exercises microphone recording.
const wav=Buffer.alloc(44+16000);wav.write('RIFF');wav.writeUInt32LE(wav.length-8,4);wav.write('WAVEfmt ',8);wav.writeUInt32LE(16,16);wav.writeUInt16LE(1,20);wav.writeUInt16LE(1,22);wav.writeUInt32LE(8000,24);wav.writeUInt32LE(16000,28);wav.writeUInt16LE(2,32);wav.writeUInt16LE(16,34);wav.write('data',36);wav.writeUInt32LE(16000,40);for(let i=0;i<8000;i++)wav.writeInt16LE(Math.round(Math.sin(i/8000*440*Math.PI*2)*2000),44+i*2);
fixtures.push({name:'driver-audio-fixture.wav',mime:'audio/wav',bytes:wav});
for(const f of fixtures)await writeFile(join(directory,f.name),f.bytes);
const feedback=async(title,expected,actual,category='bug',severity='high',evidence='')=>request(0,`${c.w}/feedback`,{id:randomUUID(),chat:c.channels['Incident room'],title,expected,actual,category,severity,evidence:`Run ${run}\n${evidence}`});
const check=async(name,fn)=>{
 const start=performance.now();
 try{await fn();checks.push({name,status:'passed',ms:performance.now()-start});console.log(`PASS ${name}`);}
 catch(error){checks.push({name,status:'failed',ms:performance.now()-start,error:String(error)});console.log(`FAIL ${name}: ${String(error)}`);await feedback(name,'Scenario assertions all pass',String(error));}
 await writeFile(join(directory,'checks.json'),JSON.stringify(checks,null,2));
};
await scenarios(c,request,check,fixtures,agentTimes);
await check('Twenty simultaneous agent requests are admitted and complete',async()=>{
 const payloads=c.employees.map(e=>({actor:e.number,chat:c.channels[e.team],id:randomUUID(),content:`${e.name}: confirm the next action during the busy period.`,agent:c.agents[e.number].id}));
 const start=performance.now();
 await Promise.all(payloads.map(p=>request(p.actor,`${c.w}/chats/${p.chat}/messages`,{id:p.id,content:'Busy-period request thread.'})));
 const results=await Promise.allSettled(payloads.map(p=>request(p.actor,`${c.w}/chats/${p.chat}/messages`,{id:randomUUID(),thread:p.id,content:p.content,agent:p.agent})));
 const accepted=payloads.filter((_,i)=>results[i].status==='fulfilled');
 await until(()=>request(0,`${c.w}/metrics`),m=>m.pending===0,'busy-period drain',60000);
 c.agentBurst={offered:20,accepted:accepted.length,rejected:20-accepted.length,elapsed_ms:performance.now()-start};
 for(const p of accepted){const h=await request(p.actor,`${c.w}/chats/${p.chat}/messages`);assert.equal(h.messages.find(m=>m.kind==='agent'&&m.thread===p.id)?.state,'complete');}
 assert.equal(accepted.length,20,'Expected the bounded queue to absorb a 20-person burst');
});
await check('Feedback stays scoped, retries are safe, and status edits reject stale revisions',async()=>{
 const id=randomUUID(),input={id,chat:c.channels.Finance,title:'Finance review example',expected:'Owner reviews supplier evidence',actual:'Awaiting supplier document',category:'usability',severity:'low',evidence:`Run ${run}: intentionally seeded feedback example`};
 await request(12,`${c.w}/feedback`,input);await request(12,`${c.w}/feedback`,input);
 assert.ok(!(await request(3,`${c.w}/feedback`)).feedback.some(f=>f.id===id));
 await request(12,`${c.w}/feedback/${id}`,{status:'investigating',revision:0},{expected:400});
 await request(0,`${c.w}/feedback/${id}`,{status:'investigating',revision:0});
 await request(0,`${c.w}/feedback/${id}`,{status:'verified',revision:0},{expected:400});
});
const limitations=[
 ['Real model quality is not tested','Evaluate judgement and factual accuracy with a separately authorized real-model study.','All replies were deterministic fixtures.','missing-feature'],
 ['External business integrations are not exercised','Validate live provider authorization and uncertain external writes separately.','The rehearsal only changes internal work records.','missing-feature'],
 ['Same-workspace transactions serialize writes','Measure contention before changing the locking strategy.','The workspace row remains a broad authorization lock; compare sustained workload latency before redesign.','architecture'],
 ['Shared runtime tools remain restricted','Extend approved internal operations to real runtime adapters with the same receipts.','Internal work execution is wired to the scripted simulation adapter; Goose still has conversation-only tools.','missing-feature'],
];
for(const [title,expected,actual,category]of limitations)await feedback(title,expected,actual,category,'medium');
const groups={};for(const s of samples)(groups[s.label]??=[]).push(s.ms);
const report={run,base,directory,created_at:new Date().toISOString(),employees:20,agents:20,inference_calls:0,checks,
 totals:{passed:checks.filter(c=>c.status==='passed').length,failed:checks.filter(c=>c.status==='failed').length,http_requests:samples.length},
 latency:{http:distribution(samples.map(s=>s.ms)),agent_request_to_observed_completion:distribution(agentTimes),by_operation:Object.fromEntries(Object.entries(groups).map(([k,v])=>[k,distribution(v)]))},
 message_burst:c.messageBurst,agent_burst:c.agentBurst,metrics:await request(0,`${c.w}/metrics`),limitations:limitations.map(([title,,actual])=>({title,actual})),
 measurement_limits:['API polling observation is not browser visibility latency.','Debug binary on one local machine; these are not production capacity guarantees.','Tail percentiles from small samples are descriptive only.','Restart, UI and recovery checks are recorded in separate artifacts.']};
await writeFile(join(directory,'report.json'),JSON.stringify(report,null,2));
await writeFile(join(directory,'requests.jsonl'),samples.map(s=>JSON.stringify(s)).join('\n')+'\n');
await writeFile(resolve(directory,'..','latest.json'),JSON.stringify({directory,run},null,2));
console.log(JSON.stringify({directory,totals:report.totals,latency:report.latency.http,agent_burst:report.agent_burst},null,2));
process.exitCode=report.totals.failed?1:0;
