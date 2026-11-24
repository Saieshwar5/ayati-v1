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
    assert.deepEqual(sidebar(),[['You',['Profile','Preferences']],['Workspace',['This workspace','Agent rules','Usage']]]);
    assert.equal(document.querySelectorAll('.settings-group button svg[aria-hidden=true]').length,5,'every page has its icon');
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
    // A person leaving doesn't choose their agents' new owner: the agents pause for an admin.
    assert.match(text,/Who takes over your Library items/);
    assert.match(text,/Paused until an admin gives them an owner1 agent: Atlas/); assert.doesNotMatch(text,/Moves to Arjun Mehta.*agent/);
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

test('Admins see seven workspace pages, Memory among them; Help and Billing are no longer pages',async()=>{
  await fixture('Admin',async({render,click,dom,reads})=>{
    assert.deepEqual(sidebar(),[['You',['Profile','Preferences']],['Workspace',['General','People','Apps','Agent rules','Memory','Usage','Audit log']]]);
    await click([...document.querySelectorAll('.settings-group button')].find(item=>item.textContent==='People'));
    assert.match(dom.window.location.search,/view=settings&section=people/);
    await render(base+'&section=usage');
    assert.match(page(),/Usage/); assert.ok(reads.some(path=>path.includes('/usage?days=30')));
    // Old links to the removed pages open the nearest page that exists.
    await render(base+'&section=billing'); assert.match(page(),/Usage/);
    await render(base+'&section=help'); assert.equal(page(),'Profile');
    assert.doesNotMatch(document.querySelector('.settings-react').textContent,/Coming later/);
  });
});

test('Links from the old tabbed Settings open the new pages',async()=>{
  await fixture('Admin',async({render,reads})=>{
    for(const [link,expected] of [
      ['&scope=workspace&section=people','People'],[ '&scope=workspace&section=access','People'],
      ['&scope=workspace&section=general','General'],['&scope=workspace&section=teams','General'],
      ['&scope=workspace&section=activity','Audit log'],['&scope=personal&section=my-account','Profile'],
      ['&scope=personal&section=help-%26-feedback','Profile'],
    ]){ await render(base+link); assert.equal(page() ?? document.querySelector('.settings-detail h1')?.textContent,expected,link); }
    await render(base.replace('view=settings','view=people')); assert.equal(page(),'People');
    await render(base.replace('view=settings','view=users')); assert.equal(page(),'People');
    // The My account page was removed (D245); its links open Settings → Profile.
    await render(base.replace('view=settings','view=account')); assert.equal(page(),'Profile');
    assert.equal(document.querySelector('.settings-group button[aria-current=page]').textContent,'Profile');
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

test('Profile groups what you edit, shows the sign-in email as text and previews the card others see',async()=>{
  await fixture('Member',async({change,click,button})=>{
    assert.equal(document.querySelector('.settings-profile > p').textContent,'How colleagues and agents see you in Northstar.');
    const form=document.querySelector('form.my-profile');
    assert.deepEqual([...document.querySelectorAll('.my-profile-edit h3')].map(node=>node.textContent),['About you','For agents','Status and presence','Account']);
    assert.deepEqual([...form.querySelectorAll('label.ui-field')].map(label=>label.firstChild.textContent),['Name','Job title','About 0 / 200','What you handle']);
    assert.equal(document.querySelectorAll('input[name=display_name]').length,1,'one name editor');
    // The email is a fact, not a disabled input.
    assert.equal(document.querySelector('input[type=email]'),null);
    assert.deepEqual([...document.querySelectorAll('.my-profile-facts > div')].map(row=>[row.querySelector('dt').textContent,row.querySelector('dd').textContent]),
      [['Sign-in email','person@test'],['Role in this workspace','Member']]);
    assert.ok(button('Change photo')); assert.equal(button('Remove photo'),undefined,'nothing to remove without a photo');
    // The card is drawn from the draft, and the bar appears only while something is unsaved.
    const card=()=>document.querySelector('.my-profile-preview-card');
    assert.equal(card().querySelector('h3').textContent,'Person');
    assert.equal(document.querySelector('.settings-save-bar'),null);
    await change(form.elements.display_name,'Priya Shah'); await change(form.elements.about,'Ask me about stock'); await change(form.elements.handles,'Stock, deliveries');
    assert.equal(card().querySelector('h3').textContent,'Priya Shah');
    assert.deepEqual([...card().querySelectorAll('.profile-card-fact > div')].map(node=>node.textContent),['Ask me about stock','Stock, deliveries','person@test']);
    assert.equal(card().querySelector('.face').textContent,'PS');
    assert.match(form.textContent,/About 18 \/ 200/);
    assert.deepEqual([...document.querySelectorAll('.settings-save-bar button')].map(node=>node.textContent),['Discard','Save']);
    await click(button('Discard'));
    assert.equal(form.elements.display_name.value,'Person'); assert.equal(document.querySelector('.settings-save-bar'),null);
    assert.equal(card().querySelector('h3').textContent,'Person');
  });
});

test('Status and “Show when I’m online” are on Profile and save at once',async()=>{
  await fixture('Member',async({click,button,writes})=>{
    const online=document.querySelector('.my-profile-edit [role=switch]');
    assert.equal(online.checked,true,'on unless you turned it off');
    await click(online);
    assert.deepEqual(writes.at(-1),{path:writes.at(-1).path,body:{desktop_alerts:true,channel_level:'mentions',approval_alerts:true,show_online:false}});
    assert.match(writes.at(-1).path,/\/preferences$/);
    assert.equal(document.querySelector('.my-profile-preview-card .presence-dot'),null,'the card stops showing you online');
    await click(button('Set a status'));
    assert.equal(document.querySelector('dialog h2').textContent,'Set a status');
  });
});

test('Failed personal profile saves preserve the draft and require explicit retry',async()=>{
  await fixture('Member',async({change,submit,writes})=>{
    const form=document.querySelector('form.my-profile');
    assert.equal(form.elements.title.value,'Buyer');
    await submit(form);
    assert.equal(writes.length,0,'nothing to save until something changes');
    await change(form.elements.display_name,'Changed name');
    await submit(form);
    assert.equal(form.elements.display_name.value,'Changed name');
    assert.match(document.querySelector('.my-profile-edit').textContent,/Save failed/);
    assert.equal(writes.length,1);
    assert.deepEqual(writes[0].body,{display_name:'Changed name',title:'Buyer',about:'',handles:''});
    assert.ok(document.querySelector('.settings-save-bar'),'the bar stays for the retry');
    // What you handle is saved with the rest, as one line for agents to read.
    await change(form.elements.handles,'Prices, discounts');
    await submit(form);
    assert.equal(writes[1].body.handles,'Prices, discounts');
    assert.equal(form.elements.handles.value,'Prices, discounts','the draft stays after a failed save');
  });
});

test('The Audit log shows who did what as sentences, with a filter for each kind',async()=>{
  await fixture('Admin',async({render,reads,button,click})=>{
    await render(base+'&section=audit-log');
    assert.equal(page(),'Audit log');
    const rows=()=>[...document.querySelectorAll('.settings-audit-day li')].map(row=>[row.querySelector('strong').textContent,row.querySelector('p').textContent]);
    assert.deepEqual(rows(),[['Person','Person removed Ravi; 1 agent moved to Person'],['Person','Person renamed the workspace to Northstar']]);
    // Entries of one day share one heading.
    assert.equal(document.querySelectorAll('.settings-audit-day h3').length,1);
    await click(button('Admin views'));
    assert.ok(reads.some(path=>path.includes('/activity?kind=admin_views')));
    assert.deepEqual(rows(),[['Meera','Meera viewed a request from a conversation they are not in (admin view)']]);
    assert.equal(document.querySelector('.settings-audit-day li').className,'admin-view');
    assert.equal(document.querySelector('.settings-audit-day li .ui-badge').textContent,'Admin view');
    assert.equal(document.querySelector('.settings-detail').textContent.includes('Simulation results'),false);
  });
});

test('Simulation results appear in the Audit log only in a simulation',async()=>{
  await fixture('Admin',async({render})=>{
    await render(base+'&section=audit-log');
    assert.ok(document.querySelector('.settings-detail').textContent.includes('Simulation results'));
  },true);
});

test('The audit log names today and yesterday, and dates older days',async()=>{
  const { dayHeading }=await import('../src/react/settings/audit-log.tsx');
  const now=new Date(2026,10,21,9,0), at=(...parts)=>new Date(...parts).getTime()/1000;
  assert.equal(dayHeading(at(2026,10,21,0,5),now),'Today');
  assert.equal(dayHeading(at(2026,10,20,23,59),now),'Yesterday');
  assert.match(dayHeading(at(2026,10,19,12),now),/19/);
  assert.match(dayHeading(at(2025,11,31,12),now),/2025/,'another year says which');
});
