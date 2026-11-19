import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format:'module', source:'export {};', shortCircuit:true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const chart = JSON.stringify({ title:'Cost per parcel', unit:'$', points:[{ label:'Courier A', value:4.1 }, { label:'Courier B', value:3.25 }] });
const pageText = `## Recommendation\n> Move to Courier B.\n\n## Cost\n\`\`\`chart\n${chart}\n\`\`\`\n\n## Side by side\n| | A | B |\n|---|---|---|\n| Late | 6% | 2% |\n\n## Switch\n\`\`\`steps\nSign | Trial contract\nPilot | One week\nSwitch | All parcels\n\`\`\``;
const reply = `Courier B is the best fit.\n\n<page title="Courier comparison">\n${pageText}\n</page>`;
const saved = (body, state='complete') => ({ id:'reply',schema:1,body,version:body.length+state,sequence:1,state,long:false,available:true,bytes:body.length,characters:body.length,format:'markdown' });
const adapter = read => ({ read,href:path=>path,response:id=>`/messages/${id}/response`,file:id=>`/files/${id}` });

test('a reply is split into its words and one page, and malformed page lines stay text',async()=>{
  const { splitPage, pageSummary, pageOutline } = await import('../src/react/pages/page-format.ts');
  const split = splitPage(`${reply}\n\nWant the plan?`);
  assert.equal(split.text,'Courier B is the best fit.\n\nWant the plan?');
  assert.deepEqual(pageSummary(split.page),{ title:'Courier comparison',sections:4,blocks:['chart','table','diagram'],complete:true,minutes:1 });
  assert.deepEqual(pageOutline(split.page.body),['Recommendation','Cost','Side by side','Switch']);
  const writing = splitPage('Here.\n<page title="Draft">\n## One\nSo far');
  assert.deepEqual([writing.text,writing.page.complete,writing.page.body],['Here.',false,'## One\nSo far']);
  for(const text of ['Short answer','Use <page title="x"> inline.','<page title="">\nx\n</page>','<page title="a<b">\nx\n</page>',`<page title="${'t'.repeat(121)}">\nx\n</page>`])
    assert.deepEqual(splitPage(text),{ text });
});

test('page blocks draw only charts and diagrams that fit their contract',async()=>{
  const { pageBlocks, chartSection } = await import('../src/react/pages/page-format.ts');
  assert.deepEqual(pageBlocks(pageText).map(block=>block.kind),['markdown','chart','markdown','steps']);
  assert.equal(chartSection(chart).variant,'bar');
  for(const bad of ['not json','{"title":"x"}','{"title":"x","points":[{"label":"a","value":"1"}]}','{"title":"x","variant":"pie","points":[{"label":"a","value":1}]}',
    '{"title":"x","variant":"donut","points":[{"label":"a","value":-1}]}','{"title":"x","series":[{"name":"a","points":[{"label":"p","value":1}]},{"name":"b","points":[{"label":"q","value":1}]}]}',
    JSON.stringify({ title:'x',points:Array.from({ length:13 },(_,i)=>({ label:`p${i}`,value:i })) })])
    assert.equal(chartSection(bad),undefined,bad);
  const broken = pageBlocks('Intro\n```chart\nnot json\n```\n```steps\nOnly one\n```\n```js\nconst page = 1;\n```');
  assert.deepEqual(broken.map(block=>block.kind),['markdown','unreadable','unreadable','markdown']);
  assert.match(broken[3].text,/const page = 1/);
  assert.match(pageBlocks('Text\n```chart\n{"title"').at(-1).text,/\{"title"/);
});

test('a reply with a page shows its words and a card that opens the page, without reading until asked',async()=>{
  let calls=0,opened=0;
  const source=adapter(async()=>{calls++;return saved(reply);});
  await reactFixture(async({render,button,click})=>{
    const {InlineResponse}=await import('../src/react/results/inline-response.tsx');
    const summary={ title:'Courier comparison',sections:4,blocks:['chart','table','diagram'],complete:true,minutes:4 };
    await render(InlineResponse,{id:'reply',text:'Courier B is the best fit.',reference:{...saved(reply),page:summary},source,openPage:()=>opened++});
    assert.match(document.body.textContent,/Courier B is the best fit\./);
    assert.match(button('Open page: Courier comparison').textContent,/4 sections · chart, table, diagram · 4 min read/);
    assert.equal(button('Expand response'),undefined);assert.equal(calls,0);
    await click(button('Open page: Courier comparison'));assert.equal(opened,1);
    // A streamed or uncompacted reply still holds the page text: it never shows as chat text.
    await render(InlineResponse,{id:'reply',text:'Working.\n<page title="Draft page">\n## One\nSo far',source,openPage:()=>opened++});
    assert.doesNotMatch(document.body.textContent,/So far|<page/);
    assert.match(button('Open page: Draft page').textContent,/Writing page…/);
    await render(InlineResponse,{id:'reply',text:reply,source});
    assert.equal(button('Open page: Courier comparison').disabled,true);
  });
});

test('the page view reads the saved reply, draws its blocks, and clears when access is revoked',async()=>{
  let notify,revoked=false,closed=0;
  const source={...adapter(async()=>{ if(revoked)throw Object.assign(new Error('Source access revoked'),{status:403}); return {...saved(reply),source_scope:'source-a'}; }),
    subscribe:fn=>{notify=fn;return()=>{notify=undefined;};}};
  await reactFixture(async({render,button,click,update})=>{
    const {ResultDialog}=await import('../src/react/results/provider.tsx');
    await render(ResultDialog,{selection:{kind:'page',id:'reply'},source,close:()=>closed++});
    const dialog=document.querySelector('dialog');
    assert.equal(dialog.querySelector('h2').textContent,'Courier comparison');
    assert.doesNotMatch(dialog.textContent,/Courier B is the best fit\./);
    assert.deepEqual([...dialog.querySelectorAll('.page-outline button')].map(node=>node.textContent),['Recommendation','Cost','Side by side','Switch']);
    assert.ok(dialog.querySelector('svg[role="img"][aria-label^="Cost per parcel"]'));
    assert.deepEqual([...dialog.querySelectorAll('.page-steps strong')].map(node=>node.textContent),['1 · Sign','2 · Pilot','3 · Switch']);
    assert.equal(dialog.querySelectorAll('table').length,2);
    // Reading only: nothing in the page asks, answers or submits.
    assert.equal(dialog.querySelectorAll('.page-body button, .page-body input, .page-body form, .page-body a').length,0);
    assert.match(new URL(dialog.querySelector('a[download]').href,'http://localhost').search,/download=true/);
    revoked=true;await update(()=>notify('source-a'));
    assert.doesNotMatch(dialog.textContent,/Move to Courier B|Cost per parcel/);assert.ok(button('Retry'));
    await click(button('Close'));assert.equal(closed,1);
  });
  assert.equal(notify,undefined);
});
