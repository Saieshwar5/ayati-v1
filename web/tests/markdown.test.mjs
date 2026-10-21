import { test } from 'node:test';
import assert from 'node:assert/strict';
import { JSDOM } from 'jsdom';
import { messageHtml } from '../src/markdown.ts';

test('agent markdown renders headings, lists, tables and literal code through a safe boundary',()=>{
 const dom=new JSDOM('<main></main>');const previous=globalThis.document;globalThis.document=dom.window.document;
 try {
  const main=document.querySelector('main');
  main.innerHTML=messageHtml('# Result\n\n**Ready** and *reviewed*\n\n- One\n- Two\n\n|Item|Count|\n|---|---|\n|Desk|2|\n\n```html\n<script>alert(1)</script>\n```\n\n[Visit](https://example.com) [Bad](javascript:alert%281%29)\n\n<img src=x onerror=alert(1)>\n\n![remote](https://tracker.example/pixel)\n\n[@Sam](mention:user:u1)');
  assert.equal(main.querySelector('h1').textContent,'Result');assert.equal(main.querySelector('strong').textContent,'Ready');assert.equal(main.querySelectorAll('li').length,2);
  assert.equal(main.querySelector('td').textContent,'Desk');assert.equal(main.querySelector('pre code').textContent.trim(),'<script>alert(1)</script>');
  assert.equal(main.querySelectorAll('a').length,1);assert.equal(main.querySelector('a').rel,'noopener noreferrer');
  assert.equal(main.querySelector('script,img,iframe,svg'),null);assert.equal(main.querySelector('.message-mention').textContent,'@Sam');
  assert.doesNotMatch(main.innerHTML, /href="javascript:/);
  assert.ok(messageHtml('Incomplete **stream').includes('Incomplete'));
 } finally {globalThis.document=previous;dom.window.close();}
});
