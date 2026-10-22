import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format:'module', source:'export {};', shortCircuit:true } : next(url,context); } });
const workspace='11111111-1111-4111-8111-111111111111';
const base=`http://localhost/w/${workspace}/?view=settings`;
async function fixture(role, run) {
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
    if(path.endsWith('/session'))data={user:{id:'person',email:'person@test',display_name:'Person'},workspaces:[{id:workspace,name:'Northstar',role}]};
    if(path.endsWith('/people'))data={role,members:[{id:'person',email:'person@test',role}],invitations:[]};
    if(path.endsWith('/settings'))data={name:'Northstar'};
    if(path.endsWith('/profile'))return {ok:false,status:400,json:async()=>({error:'Save failed'})};
    return {ok:true,status:200,json:async()=>data};
  }});
}
test('Users retain Settings and personal preferences without reading owner data',async()=>{
  await fixture('User',async({render,reads,change})=>{
    assert.equal(document.querySelector('[role=tablist]')?.textContent,'Personal');
    assert.equal(document.querySelector('h2').textContent,'My account');
    await render(base+'&scope=workspace&section=people');
    assert.equal(document.querySelector('[role=tablist]')?.textContent,'Personal');
    assert.equal(document.querySelector('[aria-label=People]'),null);
    assert.ok(!reads.some(path=>path.endsWith('/people')||path.endsWith('/settings')||path.endsWith('/activity')));
    await render(base+'&scope=personal&section=appearance');
    const radio=document.querySelector('input[value=dark]');
    await change(radio,'dark');
    await import('react').then(({act})=>act(async()=>{radio.click();}));
    assert.equal(document.documentElement.dataset.theme,'dark');
    assert.equal(localStorage.getItem('ayati.theme'),'dark');
  });
});
test('Owners use the Workspace tab; old Users links open People inside Settings',async()=>{
  await fixture('Owner',async({render,reads})=>{
    assert.equal(document.querySelector('[role=tablist]').textContent,'PersonalWorkspace');
    await render(base+'&scope=workspace&section=people');
    assert.equal(document.querySelector('[aria-label=People] h2').textContent,'People');
    assert.ok(reads.some(path=>path.endsWith('/people')));
    await render(base.replace('view=settings','view=people'));
    assert.equal(document.querySelector('[aria-label=People] h2').textContent,'People');
  });
});
test('Failed personal profile saves preserve the draft and require explicit retry',async()=>{
  await fixture('User',async({change,submit,writes})=>{
    const form=document.querySelector('form');
    await change(form.elements.display_name,'Changed name');
    await submit(form);
    assert.equal(form.elements.display_name.value,'Changed name');
    assert.match(form.textContent,/Save failed/);
    assert.equal(writes.length,1);
  });
});
