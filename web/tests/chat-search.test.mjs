import { registerHooks } from 'node:module';
registerHooks({load(url,context,next){return url.endsWith('.css')?{format:'module',source:'export {};',shortCircuit:true}:next(url,context);}});
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement, useState } from 'react';
import { reactFixture } from './react-fixture.mjs';

const workspace='11111111-1111-4111-8111-111111111111';
const url=`http://localhost/w/${workspace}/?view=chat`;
const session={user:{id:'owner',email:'owner@example.test'},workspaces:[{id:workspace,name:'Store',role:'Owner'}]};
const directory={role:'Owner',users:[{id:'owner',name:'Owner'},{id:'n',name:'Nikhil Kumar'},{id:'p',name:'Priya'}],agents:[{id:'a',name:'Stock assistant'}]};
const chats=[{id:'stock',name:'inventory',kind:'channel',description:'',users:['owner'],agents:[]},{id:'dm',name:'Nikhil',kind:'direct',description:'',users:['owner','n'],agents:[]}];
const response=(data,status=200)=>({ok:status<400,status,json:async()=>data});
async function setup(){const context=await import('../src/workspace-context.ts');await context.enterWorkspace();return (await import('../src/react/chat/search-conversations.tsx')).SearchConversations;}

test('search filters without writing, keyboard finds existing conversations, and dismissal restores focus',async()=>{
 let writes=0;const opened=[];
 await reactFixture(async({render,button,click,change,update,dom})=>{
  const Search=await setup();
  function Harness(){const[open,set]=useState(false);return createElement('div',null,createElement('button',{onClick:()=>set(true)},'Search'),open&&createElement(Search,{directory,chats,close:()=>set(false),opened:id=>{opened.push(id);set(false);}}));}
  await render(Harness);await click(button('Search'));const input=document.querySelector('input');assert.equal(document.activeElement,input);
  await change(input,'  NIKHIL  ');assert.equal(writes,0);assert.equal(document.querySelectorAll('.chat-search-results button').length,1);
  await update(()=>input.dispatchEvent(new dom.window.KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true})));
  assert.equal(document.activeElement.getAttribute('aria-label'),'Open conversation with Nikhil Kumar');
  await click(document.activeElement);assert.deepEqual(opened,['dm']);assert.equal(writes,0);assert.equal(document.activeElement,button('Search'));
  await click(button('Search'));await click(button('Agents'));assert.equal(document.querySelectorAll('.chat-search-results button').length,1);
  await change(document.querySelector('input'),'not here');assert.match(document.querySelector('.chat-search').textContent,/No matching/);
  await update(()=>document.querySelector('dialog').dispatchEvent(new dom.window.Event('cancel',{cancelable:true})));assert.equal(document.querySelector('dialog'),null);assert.equal(document.activeElement,button('Search'));
 },{url,fetch:async(path,options={})=>{if(options.method==='POST')writes++;return response(session);}});
});

test('direct creation gates double clicks, retains failed search and retries only its exact payload',async()=>{
 const bodies=[];let finish;const opened=[];
 await reactFixture(async({render,button,click,change,update})=>{
  const Search=await setup();await render(Search,{directory,chats,close:()=>{},opened:id=>opened.push(id)});
  await change(document.querySelector('input'),'Priya');const target=button('Start conversation with Priya');
  await update(()=>{target.click();target.click();});assert.equal(bodies.length,1);assert.equal(document.querySelector('fieldset').disabled,true);
  await update(()=>finish(response({error:'Save unavailable'},503)));assert.match(document.querySelector('dialog').textContent,/Save unavailable/);assert.equal(document.querySelector('input').value,'Priya');
  await click(button('Start conversation with Priya'));assert.equal(bodies.length,2);assert.deepEqual(bodies[0],bodies[1]);
  await update(()=>finish(response({error:'Still unavailable'},503)));
  await change(document.querySelector('input'),'Stock assistant');await click(button('Start conversation with Stock assistant'));assert.equal(bodies.length,3);assert.notEqual(bodies[2].id,bodies[0].id);assert.deepEqual(bodies[2].agents,['a']);assert.deepEqual(bodies[2].users,[]);
  await update(()=>finish(response({id:'created'})));assert.deepEqual(opened,['created']);
 },{url,fetch:async(path,options={})=>{if(options.method==='POST'){bodies.push(JSON.parse(options.body));return new Promise(resolve=>{finish=resolve;});}return response(session);}});
});

test('member search excludes private agents and preserves the query after a denied shared-agent request',async()=>{
 let writes=0;const opened=[];
 await reactFixture(async({render,button,click,change})=>{
  const Search=await setup();await render(Search,{directory:{...directory,role:'Member'},chats,close:()=>{},opened:id=>opened.push(id)});
  assert.equal(button('Open conversation with Private helper'),undefined);
  await click(button('Agents'));await change(document.querySelector('input'),'Stock');await click(button('Start conversation with Stock assistant'));
  assert.equal(writes,1);assert.equal(document.querySelector('input').value,'Stock');assert.match(document.querySelector('dialog').textContent,/Access denied/);assert.deepEqual(opened,[]);
 },{url,fetch:async(path,options={})=>options.method==='POST'?(writes++,response({error:'Access denied'},403)):response(session)});
});

test('late conversation creation cannot navigate a closed search or another workspace view',async()=>{
 let finish;const opened=[];
 await reactFixture(async({render,button,click,update})=>{
  const Search=await setup();await render(Search,{directory,chats,close:()=>{},opened:id=>opened.push(id)});
  await click(button('Start conversation with Priya'));await render(()=>createElement('p',null,'Other workspace'));
  await update(()=>finish(response({id:'late'})));assert.deepEqual(opened,[]);assert.match(document.body.textContent,/Other workspace/);
 },{url,fetch:async(path,options={})=>options.method==='POST'?new Promise(resolve=>{finish=resolve;}):response(session)});
});

test('direct matching reuses only exact two-party chats and opening one requires no write',async()=>{
 let writes=0;const opened=[];
 await reactFixture(async({render,button,click})=>{
  const Search=await setup();const {existingDirect}=await import('../src/react/chat/direct-conversation.ts');
  assert.equal(existingDirect([...chats,{...chats[1],id:'group-dm',users:['owner','p','other']}],{id:'p',name:'Priya',kind:'user'},'owner'),undefined);
  await render(Search,{directory,chats,close:()=>{},opened:id=>opened.push(id)});
  await click(button('Open conversation with Nikhil Kumar'));assert.deepEqual(opened,['dm']);assert.equal(writes,0);
 },{url,fetch:async(path,options={})=>{if(options.method==='POST')writes++;return response(session);}});
});
