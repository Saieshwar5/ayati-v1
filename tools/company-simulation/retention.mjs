import assert from 'node:assert/strict';
import { readFile,writeFile } from 'node:fs/promises';
import { join,resolve } from 'node:path';
import { clients,checksum } from './client.mjs';
const root=resolve('.ayati-local/company-simulation/runs');
const {directory}=JSON.parse(await readFile(join(root,'latest.json')));
const manifest=JSON.parse(await readFile(join(directory,'manifest.json')));
const {request}=clients(manifest.base,[]);
await request(0,'/api/identity/simulation/employees/0/login',{});
const metrics=await request(0,`${manifest.w}/metrics`);
const channels={};const objects={};
for(const id of Object.values(manifest.channels)) {
 let before;const messages=[];
 do {const page=await request(0,`${manifest.w}/chats/${id}/messages${before?`?before=${before}`:''}`);messages.push(...page.messages);before=page.next_before;}while(before);
 channels[id]=[...new Set(messages.map(m=>m.id))].sort();
 for(const object of messages.flatMap(m=>m.attachments??[]))objects[object.id]=checksum(await request(0,`${manifest.w}/objects/${object.id}`));
}
const snapshot={messages:metrics.messages,objects:metrics.objects,feedback:metrics.feedback,channels,checksums:objects};
const path=join(directory,'retention-before.json');
if(process.argv[2]==='capture')await writeFile(path,JSON.stringify(snapshot,null,2));
else {assert.deepEqual(snapshot,JSON.parse(await readFile(path)));await writeFile(join(directory,'retention-result.json'),JSON.stringify({passed:true,checked_at:new Date().toISOString(),...snapshot},null,2));}
console.log(`${process.argv[2]==='capture'?'Captured':'Verified'} ${snapshot.messages} messages and ${snapshot.objects} objects across restart.`);
