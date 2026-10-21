import { readFile } from 'node:fs/promises';
import { resolve,join } from 'node:path';
import { clients,randomUUID } from './client.mjs';
const {directory}=JSON.parse(await readFile(resolve('.ayati-local/company-simulation/runs/latest.json')));
const c=JSON.parse(await readFile(join(directory,'manifest.json'))),{request}=clients(c.base,[]);
await request(0,'/api/identity/simulation/employees/0/login',{});
const browser=JSON.parse(await readFile(join(directory,'browser-results.json'))),load=JSON.parse(await readFile(join(directory,'sustained-load.json')));
const observed=browser.results.find(r=>r.samples_ms)?.samples_ms??[];
const findings=[
 ['Recipient visibility is dominated by polling','performance','high','Messages should appear promptly to another employee.',`Browser visibility samples: ${observed.map(n=>Math.round(n)+' ms').join(', ')}. Open conversations poll every 2.5 seconds, even though sustained send p95 was ${Math.round(load.latency.p95_ms)} ms.`,'Evaluate permission-aware incremental delivery before simply shortening every poll interval. Keep revocation and reconnect tests.'],
 ['Queue capacity and execution capacity were incorrectly coupled','architecture','low','A 20-person burst can wait in a bounded queue without requiring 20 workers.','Baseline accepted 4/20 requests. The revised queue accepted 20/20, with four execution slots and 64 pending capacity.','Verified in two retained rehearsal reports. Do not equate this fixture result with live-model throughput.'],
 ['Short load test supports keeping the current local storage topology','architecture','low','Change the database architecture only when measured contention warrants it.',`The sustained run accepted ${load.accepted} messages with ${load.missing} missing and ${load.errors.length} send failures. PostgreSQL and bounded local file storage were sufficient for this test.`,'Keep one workspace transaction boundary for now. Test longer runs, larger files and more workspaces before introducing distributed workers or a message broker.'],
 ['Local object store needs quotas and orphan reconciliation','missing-feature','medium','Storage growth and interrupted uploads have explicit operating limits.','Per-file limits and SHA-256 checks exist. Workspace quotas, orphan reconciliation and an S3-compatible transport are not implemented.','Retain orphan bytes after uncertain metadata writes; never automatically delete evidence. Add a bounded reconciliation report before garbage collection.'],
 ['Media transport does not establish document or speech understanding','missing-feature','medium','Users can distinguish stored evidence from interpreted content.','Files, image rendering, recording and audio playback are exercised. OCR, PDF/Word parsing and speech recognition are not included.','Keep fixture interpretation labelled. Add extraction tests separately without claiming inference quality from this rehearsal.'],
 ['Short-run memory stability is not a long-duration reliability guarantee','reliability','medium','The service remains bounded over a full business day and repeated restarts.',`Memory samples stayed around ${Math.round(load.memory[0].rss_kib/1024)} MiB during a ${Math.round(load.duration_ms/1000)}-second run.`,'Next acceptance: multi-hour runs, database outage recovery, disk exhaustion and larger mixed media workloads.'],
];
for(const [title,category,severity,expected,actual,evidence]of findings)await request(0,`${c.w}/feedback`,{id:randomUUID(),chat:c.channels['Incident room'],title,category,severity,expected,actual,evidence:`Run ${c.run}\n${evidence}`});
const issues=(await request(0,`${c.w}/feedback`)).feedback;
for(const f of issues.filter(f=>f.title==='Twenty simultaneous agent requests are admitted and complete'&&f.status==='new'))await request(0,`${c.w}/feedback/${f.id}`,{status:'verified',revision:f.revision});
console.log(`Saved ${findings.length} evidence-backed findings; verified the original queue regression.`);
