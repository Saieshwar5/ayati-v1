import { randomUUID } from './client.mjs';
export async function prepare(request, roster, run) {
  const w = `/api/identity/workspaces/${roster.workspace}`;
  const employees = roster.employees;
  for (const e of employees) await request(e.number, `/api/identity/simulation/employees/${e.number}/login`, {});
  const agents = employees.map(e => ({ id: `a57e0000-0000-4000-8001-${String(e.number).padStart(12,'0')}`, name: `${e.team} · ${['Coordinator','Analyst','Assistant','Reviewer','Planner'][e.number%5]} ${e.number+1}`, role: `${e.team} work; follow explicit grants and preserve thread scope`, model: 'scripted' }));
  for (const agent of agents) await request(0,`${w}/shared-agents`,agent);
  for (const e of employees) for (const colleague of employees.filter(p => p.team === e.team)) await request(0,`${w}/chat-grants`,{user:e.user.id,agent:agents[colleague.number].id,allowed:true});
  const groups = [...new Set(employees.map(e => e.team))];
  const channels = {};
  for (const [i, team] of groups.entries()) {
    const people = employees.filter(e => e.team === team);
    await request(0,`${w}/teams`,{ id:`a57e0000-0000-4000-8002-${String(i).padStart(12,'0')}`,name:team,users:people.map(e=>e.user.id) });
    const id = randomUUID(); channels[team] = id;
    await request(0,`${w}/chats`,{id,name:`${team.toLowerCase()} · ${run}`,kind:'channel',description:`${team} daily work, decisions and handoffs`,users:people.map(e=>e.user.id),agents:people.map(e=>agents[e.number].id)});
    for (const e of people) await request(0,`${w}/chats/${id}/work-grants`,{agent:agents[e.number].id,allowed:true});
  }
  for (const name of ['Company','Order desk','Incident room']) {
    const id=randomUUID(); channels[name]=id;
    await request(0,`${w}/chats`,{id,name:`${name} · ${run}`,kind:'channel',description:name==='Company'?'Company announcements':'Cross-team coordination',users:employees.map(e=>e.user.id),agents:[]});
  }
  await request(0,`${w}/chats/${channels.Company}/settings`,{visibility:'public',announcements:true});
  const dms=[];
  for (let i=0;i<20;i+=2) {
    const payload={id:randomUUID(),name:`${employees[i].name} / ${employees[i+1].name}`,kind:'direct',description:'',users:[employees[i+1].user.id],agents:[]};
    const first=await request(i,`${w}/chats`,payload);
    const second=await request(i+1,`${w}/chats`,{...payload,id:randomUUID(),users:[employees[i].user.id]});
    if(first.id!==second.id) throw new Error('DM identity was duplicated');
    dms.push({id:first.id,left:i,right:i+1});
  }
  return {w,employees,agents,channels,dms};
}
