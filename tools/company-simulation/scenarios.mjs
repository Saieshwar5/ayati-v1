import assert from 'node:assert/strict';
import { randomUUID, until, checksum, delay } from './client.mjs';
import { performance } from 'node:perf_hooks';
export async function scenarios(c, request, check, fixtures, timings) {
  const {w,employees,agents,channels,dms}=c;
  const send=(actor,chat,content,extra={})=>request(actor,`${w}/chats/${chat}/messages`,{id:randomUUID(),content,...extra});
  const history=(actor,chat)=>request(actor,`${w}/chats/${chat}/messages`);
  const agentReply=async(actor,chat,text,agent,extra={})=>{
    const id=randomUUID(),start=performance.now();
    await request(actor,`${w}/chats/${chat}/messages`,{id,content:text,agent,...extra});
    const h=await until(()=>history(actor,chat),h=>h.messages.some(m=>m.kind==='agent' && m.thread===(extra.thread??id) && m.state!=='queued'),'agent completion');
    const reply=h.messages.find(m=>m.kind==='agent' && m.thread===(extra.thread??id));
    timings.push(performance.now()-start); return {id,reply};
  };
  await check('Morning announcements and department check-ins', async()=>{
    await send(0,channels.Company,'Good morning. Today we need to fulfil the 20-desk order for Northstar Studio. Please keep approvals and delivery evidence in the relevant threads.');
    await Promise.all(employees.map(e=>send(e.number,channels[e.team],`Good morning, ${e.name} here. I am available for ${e.team.toLowerCase()} work today.`)));
    for(const dm of dms){await send(dm.left,dm.id,'Can you cover my customer follow-up while I check the order?');await send(dm.right,dm.id,'Yes. Please send the handoff details here.');}
  });
  await check('Quotation produces an actual draft; approval and completion are enforced',async()=>{
    const {reply}=await agentReply(3,channels.Sales,'Please prepare the quotation for 20 desks. [quote]',agents[3].id);
    assert.equal(reply.state,'complete');
    let work=(await request(3,`${w}/work`)).work.find(t=>t.id===reply.id);
    assert.ok(work);assert.equal(work.details.total_minor,8000000);assert.equal(work.status,'draft');
    const blocked=await agentReply(3,channels.Sales,`Complete this quotation now. [complete:${work.id}]`,agents[3].id);
    assert.equal(blocked.reply.state,'failed');
    await request(4,`${w}/work/${work.id}`,{status:'approved',assignee:employees[3].user.id,revision:work.revision},{expected:400});
    await request(3,`${w}/work/${work.id}`,{status:'approved',assignee:employees[3].user.id,revision:work.revision});
    await request(3,`${w}/work/${work.id}`,{status:'approved',assignee:employees[3].user.id,revision:work.revision},{expected:400});
    const completed=await agentReply(3,channels.Sales,`Approval is recorded; finish the internal quotation. [complete:${work.id}]`,agents[3].id);
    assert.equal(completed.reply.state,'complete');
    work=(await request(3,`${w}/work`)).work.find(t=>t.id===reply.id);assert.equal(work.status,'completed');
    await send(3,channels['Order desk'],`Quotation ${work.id} has been approved. Operations: please check the stock before we confirm delivery.`);
  });
  await check('Every agent receives an explicit, correctly threaded request',async()=>{
    for(const employee of employees){const {reply}=await agentReply(employee.number,channels[employee.team],`${employee.name}: please confirm today's responsibility and keep it in this thread.`,agents[employee.number].id);assert.equal(reply.state,'complete');assert.equal(reply.author,agents[employee.number].id);}
  });
  await check('Documents, image and voice attachment bytes survive storage and are scoped',async()=>{
    const list=[];
    for(const fixture of fixtures){
      const object=await request(7,`${w}/chats/${channels.Operations}/objects?name=${encodeURIComponent(fixture.name)}`,undefined,{raw:fixture.bytes,mime:fixture.mime});
      assert.equal(object.checksum,checksum(fixture.bytes));
      await request(8,`${w}/objects/${object.id}`,undefined,{expected:400});
      await send(7,channels.Operations,`Delivery evidence: ${fixture.name}`,{attachments:[object.id]});
      const bytes=await request(8,`${w}/objects/${object.id}`);assert.equal(checksum(bytes),object.checksum);
      await request(3,`${w}/objects/${object.id}`,undefined,{expected:400});
      await request(7,`${w}/chats/${channels['Order desk']}/messages`,{id:randomUUID(),content:'Wrong channel attachment',attachments:[object.id]},{expected:400});
      list.push(object);
    }
    assert.equal((await history(7,channels.Operations)).messages.flatMap(m=>m.attachments??[]).length,list.length);
    const {reply}=await agentReply(7,channels.Operations,'Check the stock and explain the shortage. [stock]',agents[7].id);assert.equal(reply.state,'complete');
    await send(7,channels['Order desk'],'12 desks are available; 8 need replenishment. Purchasing is coordinating the shortage.');
    await agentReply(8,channels.Operations,'I have uploaded the driver recording and signed note. [delivery]',agents[8].id);
  });
  await check('Private finance, announcement posting and forged-thread boundaries',async()=>{
    await send(12,channels.Finance,'FINANCE_PRIVATE_CANARY: supplier bank detail review is still pending.');
    await request(3,`${w}/chats/${channels.Finance}/messages`,undefined,{expected:400});
    await request(4,`${w}/chats/${channels.Company}/messages`,{id:randomUUID(),content:'Unauthorized announcement'},{expected:400});
    const root=randomUUID();await request(3,`${w}/chats/${channels.Sales}/messages`,{id:root,content:'Sales-only root'});
    await request(7,`${w}/chats/${channels.Operations}/messages`,{id:randomUUID(),thread:root,content:'Wrong-thread reply'},{expected:400});
    const {reply}=await agentReply(3,channels.Sales,'Summarize this thread only. [summary]',agents[3].id);assert.ok(!reply.content.includes('FINANCE_PRIVATE_CANARY'));
  });
  await check('Message retry is idempotent and altered retries fail',async()=>{
    const payload={id:randomUUID(),content:'Customer confirmed the delivery window: 2–4 PM.'};
    await Promise.all(Array.from({length:8},()=>request(15,`${w}/chats/${channels.Support}/messages`,payload)));
    assert.equal((await history(15,channels.Support)).messages.filter(m=>m.id===payload.id).length,1);
    await request(15,`${w}/chats/${channels.Support}/messages`,{...payload,content:'Changed payload'},{expected:400});
  });
  await check('Channel membership revocation removes history and file access',async()=>{
    const members=employees.filter(e=>e.team==='Operations').map(e=>e.user.id);
    const object=(await history(7,channels.Operations)).messages.flatMap(m=>m.attachments??[])[0];assert.ok(object);
    await request(0,`${w}/chats/${channels.Operations}/members`,{users:[employees[0].user.id,...members.filter(id=>id!==employees[11].user.id)]});
    await request(11,`${w}/chats/${channels.Operations}/messages`,undefined,{expected:400});
    await request(11,`${w}/objects/${object.id}`,undefined,{expected:400});
    await request(0,`${w}/chats/${channels.Operations}/members`,{users:[employees[0].user.id,...members]});
    assert.ok((await history(11,channels.Operations)).messages.length);
  });
  await check('Shared work grant can be revoked independently of chat access',async()=>{
    await request(0,`${w}/chats/${channels.Sales}/work-grants`,{agent:agents[4].id,allowed:false});
    const {reply}=await agentReply(4,channels.Sales,'Prepare an unauthorized quotation. [quote]',agents[4].id);assert.equal(reply.state,'failed');
    await request(0,`${w}/chats/${channels.Sales}/work-grants`,{agent:agents[4].id,allowed:true});
  });
  await check('History pagination preserves every burst message without duplicates',async()=>{
    const ids=[];const start=performance.now();
    for(let batch=0;batch<15;batch++) await Promise.all(employees.map(async e=>{const id=randomUUID();ids.push(id);await request(e.number,`${w}/chats/${channels['Order desk']}/messages`,{id,content:`Order status ${batch+1}: ${e.name} checked their handoff and confirmed the next action.`});}));
    let before,seen=new Set();do{const page=await request(0,`${w}/chats/${channels['Order desk']}/messages${before?`?before=${before}`:''}`);for(const m of page.messages)seen.add(m.id);before=page.next_before;}while(before);
    for(const id of ids)assert.ok(seen.has(id));
    c.messageBurst={messages:ids.length,elapsed_ms:performance.now()-start};
  });
  await check('Customer exception and end-of-day handoff',async()=>{
    await send(15,channels.Support,'The customer reports a damaged corner on one desk. Please keep this case open until Operations confirms a replacement.');
    await agentReply(15,channels.Support,'Review the complaint and request a human decision. [support]',agents[15].id);
    await send(7,channels['Incident room'],'Operations will inspect the damage evidence tomorrow morning. The delivery is not yet fully resolved.');
    await send(15,channels['Incident room'],'Support owns the customer update. We will not close the case until the replacement is confirmed.');
    await agentReply(0,channels.Management,'Prepare the end-of-day review. [summary]',agents[0].id);
    await delay(100);
  });
}
