import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format:'module', source:'export {};', shortCircuit:true } : next(url,context); } });
const workspace='11111111-1111-4111-8111-111111111111';
const base=`http://localhost/w/${workspace}/?view=settings`;
let refuseLeave=false;
async function fixture(role, run, simulation=false) {
  const reads=[], writes=[];
  await reactFixture(async context => {
    const { enterWorkspace }=await import('../src/workspace-context.ts'); await enterWorkspace();
    const { SettingsPage }=await import('../src/react/settings/settings-page.tsx');
    const render=href => context.render(SettingsPage,{href});
    await render(base);
    await run({...context,render,reads,writes});
  },{url:base,fetch:async(path,options={})=>{
    const body=options.body ? JSON.parse(options.body) : undefined;
    if(body)writes.push({path,body});else reads.push(path);
    let data={};
    if(path.endsWith('/session'))data={user:{id:'person',email:'person@test',display_name:'Person'},workspaces:[{id:workspace,name:'Northstar',role}],simulation};
    if(path.endsWith('/people'))data={role,members:[{id:'person',email:'person@test',role}],invitations:[]};
    if(path.endsWith('/settings'))data={name:'Northstar',timezone:null,colour:null,approval_backup_hours:4,models:[],default_model:null,delete_at_ms:null,delete_requested_by:null};
    if(path.endsWith('/preferences'))data=body??{desktop_alerts:true,channel_level:'mentions',approval_alerts:true};
    if(path.includes('/activity?'))data=path.includes('kind=admin_views')?{activity:[{id:3,actor:'Meera',action:'activity_private_viewed',kind:'admin_views',sentence:'viewed a request from a conversation they are not in (admin view)',at:1}],next_before:null}
      :{activity:[{id:2,actor:'Person',action:'membership_changed',kind:'people',sentence:'removed Ravi; 1 agent moved to Person',at:1},{id:1,actor:'Person',action:'workspace_renamed',kind:'other',sentence:'renamed the workspace to Northstar',at:1}],next_before:null};
    if(path.endsWith('/simulation/reports'))data={reports:[]};
    if(path.endsWith('/feedback'))data={feedback:[]};
    if(path.endsWith('/chats'))data={chats:[]};
    if(path.endsWith('/chat-directory'))data={role,users:[{id:'person',name:'Person',title:'Buyer',about:'',photo:null,role},{id:'lead',name:'Arjun Mehta',email:'arjun@test',role:'Admin'}],agents:[]};
    if(path.endsWith('/my-profile'))return {ok:false,status:400,json:async()=>({error:'Save failed'})};
    if(path.endsWith('/responsibilities'))data={agents:[{id:'a1',name:'Atlas'}],private_agents:1,library_items:0,kept_library_items:0,schedules:0,event_triggers:0,invitations:0};
    if(body&&path.endsWith('/members/person')&&refuseLeave)return {ok:false,status:400,json:async()=>({error:'A workspace must keep at least one admin'})};
    return {ok:true,status:200,json:async()=>data};
  }});
}
const sidebar=()=>[...document.querySelectorAll('.settings-group')].map(group=>[group.querySelector('.settings-group-title').textContent,[...group.querySelectorAll('button')].map(item=>item.textContent)]);
const page=()=>document.querySelector('.settings-detail h2')?.textContent;

test('Members see their own pages and This workspace, and never read admin data',async()=>{
  await fixture('Member',async({render,reads,change})=>{
    assert.deepEqual(sidebar(),[['You',['Profile','Preferences','Help']],['Workspace',['This workspace']]]);
    assert.equal(page(),'Profile');
    assert.match(document.querySelector('.settings-heading p').textContent,/Northstar · Member/);
    await render(base+'&scope=workspace&section=people');
    assert.equal(page(),'Profile','admin pages fall back for members');
    assert.ok(!reads.some(path=>path.endsWith('/people')||path.endsWith('/settings')||path.includes('/activity')));
    await render(base+'&section=this-workspace');
    assert.equal(page(),'This workspace');
    assert.deepEqual([...document.querySelectorAll('.settings-admins strong')].map(node=>node.textContent),['Arjun Mehta']);
    await render(base+'&scope=personal&section=appearance');
    assert.equal(page(),'Preferences');
    const radio=document.querySelector('input[value=dark]');
    await change(radio,'dark');
    await import('react').then(({act})=>act(async()=>{radio.click();}));
    assert.equal(document.documentElement.dataset.theme,'dark');
    assert.equal(localStorage.getItem('ayati.theme'),'dark');
  });
});

test('Members see counts and what admins can see, and leave naming who takes over',async()=>{
  await fixture('Member',async({render,click,change,reads,writes,navigation})=>{
    await render(base+'&section=this-workspace');
    assert.match(document.querySelector('.settings-workspace-card p').textContent,/^You are a Member · 2 people · 0 agents$/);
    assert.match(document.querySelector('.settings-detail').textContent,/What admins can see.*never other people’s private channels or direct messages/);
    await click(document.querySelector('.settings-leave button'));
    assert.equal(document.querySelector('dialog h2').textContent,'Leave Northstar?');
    assert.ok(reads.some(path=>path.endsWith('/members/person/responsibilities')));
    const select=document.querySelector('dialog select');
    assert.equal(select.value,'lead','an admin takes over by default');
    assert.deepEqual([...select.options].map(option=>option.textContent),['Arjun Mehta (admin)']);
    const text=document.querySelector('dialog').textContent;
    assert.match(text,/Moves to Arjun Mehta.*1 agent: Atlas/); assert.match(text,/Paused for review.*1 only-me agent/);
    await change(select,'lead');
    const leave=document.querySelector('dialog .settings-remove');
    await click(leave); await click(leave);
    assert.equal(writes.length,1,'one request for a double click');
    assert.match(writes[0].path,/\/members\/person$/); assert.deepEqual(writes[0].body,{role:null,handover_to:'lead'});
    assert.deepEqual(navigation,['/?view=account&choose=1']);
  });
});

test('A refused leave keeps the dialog open with the reason',async()=>{
  refuseLeave=true;
  try {
    await fixture('Member',async({render,click,navigation})=>{
      await render(base+'&section=this-workspace');
      await click(document.querySelector('.settings-leave button'));
      await click(document.querySelector('dialog .settings-remove'));
      assert.ok(document.querySelector('dialog').open);
      assert.match(document.querySelector('dialog').textContent,/at least one admin/);
      assert.deepEqual(navigation,[]);
    });
  } finally { refuseLeave=false; }
});

test('Admins see the workspace pages; Usage and Billing are marked as coming later',async()=>{
  await fixture('Admin',async({render,click,dom})=>{
    assert.deepEqual(sidebar(),[['You',['Profile','Preferences','Help']],['Workspace',['Workspace','People','Connections','UsageComing later','BillingComing later','Audit log']]]);
    await click([...document.querySelectorAll('.settings-group button')].find(item=>item.textContent==='People'));
    assert.match(dom.window.location.search,/view=settings&section=people/);
    await render(base+'&section=usage');
    assert.match(page(),/Usage/); assert.match(document.querySelector('.settings-detail').textContent,/not built yet/);
  });
});

test('Links from the old tabbed Settings open the new pages',async()=>{
  await fixture('Admin',async({render,reads})=>{
    for(const [link,expected] of [
      ['&scope=workspace&section=people','People'],[ '&scope=workspace&section=access','People'],
      ['&scope=workspace&section=general','Workspace'],['&scope=workspace&section=teams','Workspace'],
      ['&scope=workspace&section=activity','Audit log'],['&scope=personal&section=my-account','Profile'],
      ['&scope=personal&section=help-%26-feedback','Help'],
    ]){ await render(base+link); assert.equal(page() ?? document.querySelector('.settings-detail h1')?.textContent,expected,link); }
    await render(base.replace('view=settings','view=people')); assert.equal(page(),'People');
    await render(base.replace('view=settings','view=users')); assert.equal(page(),'People');
    await render(base+'#theme'); assert.equal(page(),'Preferences');
    assert.ok(reads.some(path=>path.endsWith('/people')));
  });
});

test('Phones show the list first and a page with a way back',async()=>{
  await fixture('Admin',async({render,click,dom})=>{
    assert.equal(document.querySelector('.settings-react').dataset.showing,'list');
    await render(base+'&section=people');
    assert.equal(document.querySelector('.settings-react').dataset.showing,'page');
    await click(document.querySelector('.settings-back'));
    assert.equal(dom.window.location.search,'?view=settings');
  });
});

test('Failed personal profile saves preserve the draft and require explicit retry',async()=>{
  await fixture('Member',async({change,submit,writes})=>{
    const form=document.querySelector('form.my-profile');
    assert.equal(form.querySelector('input[maxlength="80"]:not([required])').value,'Buyer');
    await change(form.elements.display_name,'Changed name');
    await submit(form);
    assert.equal(form.elements.display_name.value,'Changed name');
    assert.match(form.textContent,/Save failed/);
    assert.equal(writes.length,1);
    assert.deepEqual(writes[0].body,{display_name:'Changed name',title:'Buyer',about:''});
  });
});

test('The Audit log shows who did what as sentences, with a filter for each kind',async()=>{
  await fixture('Admin',async({render,reads,button,click})=>{
    await render(base+'&section=audit-log');
    assert.equal(page(),'Audit log');
    const rows=()=>[...document.querySelectorAll('.settings-audit-table tr')].map(row=>[...row.children].slice(1).map(cell=>cell.textContent));
    assert.deepEqual(rows(),[['Person','removed Ravi; 1 agent moved to Person'],['Person','renamed the workspace to Northstar']]);
    await click(button('Admin views'));
    assert.ok(reads.some(path=>path.includes('/activity?kind=admin_views')));
    assert.deepEqual(rows(),[['Meera','viewed a request from a conversation they are not in (admin view)']]);
    assert.equal(document.querySelector('.settings-audit-table tr').className,'admin-view');
    assert.equal(document.querySelector('.settings-detail').textContent.includes('Simulation results'),false);
  });
});

test('Simulation results appear in the Audit log only in a simulation',async()=>{
  await fixture('Admin',async({render})=>{
    await render(base+'&section=audit-log');
    assert.ok(document.querySelector('.settings-detail').textContent.includes('Simulation results'));
  },true);
});
