import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
const saved = (body, state = 'draft') => ({ schema:1, id:'r', body, state, version:body, sequence:body.length, long:true, available:true, bytes:body.length, characters:body.length, format:'markdown' });
const source = read => ({ read, href:path=>path, response:id=>`/messages/${id}/response`, file:id=>`/files/${id}` });
test('reader freezes its saved draft, updates only explicitly, downloads its complete version and clears denied content', async () => {
  let check, current = saved('First saved draft'), calls = 0, denied = false;
  const adapter = { ...source(async () => { calls++; if (denied) throw Object.assign(new Error('Source access revoked'),{status:403}); return current; }), subscribe: fn => { check = fn; return () => { check = undefined; }; } };
  await reactFixture(async ({ render, button, click, update }) => {
    const { ResponseReader } = await import('../src/react/results/reader.tsx');
    await render(ResponseReader, { id:'r', source:adapter, close:()=>{} });
    assert.match(document.querySelector('.result-body').textContent.trim(),/First saved draft/);
    assert.equal(document.querySelector('a[download]'),null);
    current = saved('Final completed body','complete'); await update(()=>check());
    assert.equal(document.querySelector('.result-body').textContent.trim(),'First saved draft');
    assert.match(document.body.textContent,/complete response is ready/);
    await click(button('Load latest saved response'));
    assert.equal(document.querySelector('.result-body').textContent.trim(),'Final completed body');
    const url = new URL(document.querySelector('a[download]').href);
    assert.equal(url.searchParams.get('version'),current.version); assert.equal(url.searchParams.get('download'),'true');
    denied = true; await update(()=>check()); assert.equal(document.querySelector('.result-body'),null);
    assert.match(document.body.textContent,/Source access revoked/); assert.ok(calls >= 4);
  });
  assert.equal(check,undefined);
});
test('old source and unmounted reads cannot replace current content', async () => {
  let resolveOld;
  const adapter = source(path => path.includes('/old/') ? new Promise(resolve=>{ resolveOld = resolve; }) : Promise.resolve(saved('Current saved response')));
  await reactFixture(async ({ render, update }) => {
    const { ResponseReader } = await import('../src/react/results/reader.tsx');
    await render(ResponseReader,{id:'old',source:adapter,close:()=>{}});
    await render(ResponseReader,{id:'new',source:adapter,close:()=>{}});
    await update(()=>resolveOld(saved('PRIVATE_OLD_BODY')));
    assert.equal(document.querySelector('.result-body').textContent.trim(),'Current saved response');
    assert.doesNotMatch(document.body.textContent,/PRIVATE_OLD_BODY/);
  });
});
test('file viewer requests full bytes only through explicit image opening and offers the exact original', async () => {
  const calls = []; const file={id:'image',name:'Business image.png',mime:'image/png',size:1024,checksum:'exacthash'};
  await reactFixture(async ({ render, button, click }) => {
    const { FileViewer } = await import('../src/react/results/file-viewer.tsx');
    await render(FileViewer,{id:file.id,name:file.name,source:source(async path=>{calls.push(path);return file;}),close:()=>{}});
    assert.ok(calls.every(path=>path.includes('metadata=true')));
    assert.match(document.querySelector('img').getAttribute('src'),/inline=true&version=exacthash/);
    await click(button('Zoom in')); assert.equal(document.querySelector('output').textContent,'125%');
    await click(button('Fit')); assert.equal(document.querySelector('output').textContent,'Fit');
    assert.match(document.querySelector('a[download]').getAttribute('href'),/version=exacthash/);
  });
});
test('preview bounds Unicode and structural long responses without summarizing or loading a model', async () => {
  const { responsePreview } = await import('../src/react/results/model.ts');
  const text='你好🦀'.repeat(500); const preview=responsePreview(text);
  assert.equal([...preview.text].length,320); assert.equal(preview.long,true); assert.ok(text.startsWith(preview.text));
  assert.equal(responsePreview('A short answer').long,false);
  assert.equal(responsePreview('line\n'.repeat(10)).long,true);
});

test('source-scoped invalidation clears an open reader and fences late pre-revocation metadata',async()=>{
 let check,old;let calls=0,revoked=false;
 const adapter={...source(async path=>{
  calls++;if(revoked)throw Object.assign(new Error('Event source revoked'),{status:403});
  if(path.includes('metadata=true'))return new Promise(resolve=>{old=resolve;});
  return {...saved('PRIVATE_EVENT_BODY','complete'),source_scope:'event-source'};
 }),subscribe:fn=>{check=fn;return()=>{check=undefined;};}};
 await reactFixture(async({render,update})=>{
  const {ResponseReader}=await import('../src/react/results/reader.tsx');await render(ResponseReader,{id:'r',source:adapter,close:()=>{}});
  const initial=calls;await update(()=>check('other-source'));assert.match(document.body.textContent,/PRIVATE_EVENT_BODY/);assert.equal(calls,initial);
  await update(()=>check());assert.equal(calls,initial+1);revoked=true;
  await update(()=>check('event-source'));assert.equal(document.querySelector('.result-body'),null);assert.match(document.body.textContent,/Event source revoked/);
  await update(()=>old({...saved('PRIVATE_STALE_EVENT_BODY','complete'),source_scope:'event-source'}));assert.doesNotMatch(document.body.textContent,/PRIVATE_STALE|PRIVATE_EVENT_BODY/);
 });
});
