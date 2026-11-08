// Runs inside the test computer: CDP to the local Chromium on 9222 through the browser
// target with flat sessions. Usage: node inside_cdp.mjs open|check|close ; prints one JSON line.
// Needs the `ws` package in the computer (npm install ws).
import { writeFileSync } from 'node:fs';
import WebSocket from 'ws';
setTimeout(() => { console.log(JSON.stringify({ error: 'gave up after 40 s' })); process.exit(2); }, 40000);
const action = process.argv[2];
const version = await (await fetch('http://127.0.0.1:9222/json/version')).json();
const ws = new WebSocket(version.webSocketDebuggerUrl);
await new Promise((ok, fail) => { ws.on('open', ok); ws.on('error', fail); });
let n = 0; const waiting = new Map(); const listeners = [];
ws.on('message', data => {
  const msg = JSON.parse(data);
  if (msg.id && waiting.has(msg.id)) { waiting.get(msg.id)(msg.result ?? msg.error); waiting.delete(msg.id); }
  else if (msg.method) listeners.forEach(l => l(msg));
});
let sid;
const call = (method, params = {}) => new Promise(ok => { const id = ++n; waiting.set(id, ok); ws.send(JSON.stringify({ id, method, params, ...(sid ? { sessionId: sid } : {}) })); });
const sleep = ms => new Promise(r => setTimeout(r, ms));
const value = async expression => { const r = await call('Runtime.evaluate', { expression, returnByValue: true }); return r.exceptionDetails ? 'ERROR ' + (r.exceptionDetails.exception?.description || r.exceptionDetails.text) : r.result?.value ?? null; };
// Attach to the agent's existing page, or open one.
const targets = (await call('Target.getTargets')).targetInfos.filter(t => t.type === 'page');
const targetId = targets.length ? targets[0].targetId : (await call('Target.createTarget', { url: 'about:blank' })).targetId;
sid = (await call('Target.attachToTarget', { targetId, flatten: true })).sessionId;
const out = {};
if (action === 'open') {
  await call('Page.enable');
  await call('Page.navigate', { url: 'https://example.com' });
  await sleep(2000);
  out.url = await value('location.href');
  await value("document.cookie='cuckoo_login=kept; max-age=86400'");
  const shot = await call('Page.captureScreenshot', { format: 'png' });
  writeFileSync('/tmp/shot.png', Buffer.from(shot.data, 'base64'));
  out.screenshot_kb = Math.round(Buffer.from(shot.data, 'base64').length / 1024);
  const frames = [];
  listeners.push(msg => { if (msg.method === 'Page.screencastFrame') { frames.push(msg.params); ws.send(JSON.stringify({ id: ++n, method: 'Page.screencastFrameAck', params: { sessionId: msg.params.sessionId }, sessionId: sid })); } });
  const started = Date.now();
  await call('Page.startScreencast', { format: 'jpeg', quality: 60, maxWidth: 1280, maxHeight: 800 });
  await value("window.__k=0; setInterval(()=>{document.body.style.background=['#fff','#eef','#efe'][window.__k++%3]}, 100)");
  while (frames.length < 20 && Date.now() - started < 5000) await sleep(25);
  await call('Page.stopScreencast');
  const seconds = (Date.now() - started) / 1000;
  out.screencast = { frames: frames.length, seconds, frames_per_second: Math.round(frames.length / seconds), average_frame_kb: frames.length ? Math.round(frames.reduce((s, f) => s + f.data.length * 0.75, 0) / frames.length / 1024) : 0 };
  // A person's click and typing, sent as the hand-over view would send them.
  out.insert_raw = JSON.stringify(await call('Runtime.evaluate', { expression: "const i=document.createElement('input'); i.id='cuckoo_typed'; i.style.cssText='position:fixed;left:10px;top:10px;width:300px;height:40px;z-index:9'; document.body.appendChild(i); 'added'", returnByValue: true })).slice(0, 200);
  for (const type of ['mousePressed', 'mouseReleased']) await call('Input.dispatchMouseEvent', { type, x: 40, y: 30, button: 'left', clickCount: 1 });
  await call('Input.insertText', { text: 'typed by a person' });
  out.typed_raw = JSON.stringify(await call('Runtime.evaluate', { expression: "document.getElementById('cuckoo_typed') ? document.getElementById('cuckoo_typed').value : 'missing'", returnByValue: true })).slice(0, 200);
  // What the agent reads instead of pixels.
  out.page_text = (await value('document.body.innerText')).slice(0, 60);
} else if (action === 'close') {
  sid = undefined;
  out.closed = JSON.stringify(await call('Browser.close'));
} else {
  out.url = await value('location.href');
  out.cookie = await value('document.cookie');
  out.typed_value_still_there = await value("document.getElementById('cuckoo_typed') ? document.getElementById('cuckoo_typed').value : null");
}
console.log(JSON.stringify(out));
process.exit(0);
