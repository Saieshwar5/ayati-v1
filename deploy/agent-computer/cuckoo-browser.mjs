// Cuckoo's browser helper, run inside an agent's computer (runtime/agent-computers.md).
// The controller uploads it and runs one action at a time:
//   node cuckoo-browser.mjs <open|read|click|type|choose|press|screenshot|live> '<json>'
// Each action prints one JSON line. `live` instead prints one line per screencast frame
// and reads a person's input as JSON lines on stdin until stdin closes.
// Chromium is started on first use and listens for CDP on 127.0.0.1 only.
import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { createInterface } from 'node:readline';

const PORT = Number(process.env.CUCKOO_BROWSER_PORT || 9222);
const home = process.env.HOME || '/home/user';
const [action = 'read', raw = '{}'] = process.argv.slice(2);
const input = JSON.parse(raw);
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const done = (result, code = 0) => { process.stdout.write(`${JSON.stringify(result)}\n`, () => process.exit(code)); };
const fail = message => done({ error: message }, 1);
if (action !== 'live') setTimeout(() => fail('The browser took too long to answer.'), 50_000).unref();

/** The WebSocket client: Node's own when it has one, otherwise the image's `ws`. */
function socketClass() {
  if (globalThis.WebSocket) return globalThis.WebSocket;
  return createRequire('/opt/cuckoo/')('ws');
}

async function version() {
  try { return await (await fetch(`http://127.0.0.1:${PORT}/json/version`)).json(); } catch { return undefined; }
}

/** The running browser, started with its own profile when it isn't running. */
async function browser() {
  let found = await version();
  if (found) return found;
  const profile = `${home}/.cuckoo/browser-profile`;
  mkdirSync(profile, { recursive: true });
  const flags = ['--headless=new', '--no-sandbox', '--disable-dev-shm-usage', '--no-zygote', '--disable-gpu',
    `--remote-debugging-port=${PORT}`, '--remote-debugging-address=127.0.0.1', '--remote-allow-origins=*',
    `--user-data-dir=${profile}`, '--window-size=1280,800', '--no-first-run', '--no-default-browser-check', 'about:blank'];
  spawn(process.env.CUCKOO_CHROMIUM || 'chromium', flags, { detached: true, stdio: 'ignore' }).unref();
  for (let i = 0; i < 100 && !found; i++) { await sleep(150); found = await version(); }
  if (!found) throw new Error('The browser could not be started.');
  return found;
}

/** One CDP connection, attached to the agent's single page. */
async function connect() {
  const Socket = socketClass();
  const ws = new Socket((await browser()).webSocketDebuggerUrl);
  await new Promise((ok, no) => { ws.onopen = ok; ws.onerror = () => no(new Error('The browser did not answer.')); });
  let next = 0; let session;
  const waiting = new Map(); const listeners = [];
  ws.onmessage = event => {
    const message = JSON.parse(typeof event.data === 'string' ? event.data : event.data.toString());
    if (message.id && waiting.has(message.id)) {
      const { ok, no } = waiting.get(message.id); waiting.delete(message.id);
      if (message.error) no(new Error(message.error.message)); else ok(message.result);
    } else if (message.method) listeners.forEach(listen => listen(message));
  };
  const call = (method, params = {}) => new Promise((ok, no) => {
    const id = ++next; waiting.set(id, { ok, no });
    ws.send(JSON.stringify({ id, method, params, ...(session ? { sessionId: session } : {}) }));
  });
  const pages = (await call('Target.getTargets')).targetInfos.filter(target => target.type === 'page');
  const target = pages.length ? pages[0].targetId : (await call('Target.createTarget', { url: 'about:blank' })).targetId;
  session = (await call('Target.attachToTarget', { targetId: target, flatten: true })).sessionId;
  await call('Page.enable');
  return { call, listeners, close: () => ws.close() };
}

async function evaluate(cdp, expression) {
  const result = await cdp.call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description?.split('\n')[0] || 'The page refused it.');
  return result.result?.value;
}

/** Waits until the page has loaded, or a while has passed. */
async function settle(cdp, longest = 15_000) {
  const until = Date.now() + longest;
  await sleep(300);
  while (Date.now() < until) {
    if (await evaluate(cdp, 'document.readyState').catch(() => 'loading') === 'complete') break;
    await sleep(200);
  }
  await sleep(300);
}

// What the agent reads: the page's text and its numbered controls. Numbers are stored
// on the elements, so the next click or typing finds the same one.
const READ = `(() => {
  document.querySelectorAll('[data-cuckoo-ref]').forEach(e => e.removeAttribute('data-cuckoo-ref'));
  const pick = 'a[href],button,input:not([type=hidden]),select,textarea,summary,[role=button],[role=link],[role=checkbox],[role=radio],[role=tab],[role=menuitem],[role=option],[contenteditable=true]';
  const elements = [];
  for (const el of document.querySelectorAll(pick)) {
    const box = el.getBoundingClientRect(); const style = getComputedStyle(el);
    if (box.width < 1 || box.height < 1 || style.visibility === 'hidden' || style.display === 'none') continue;
    if (elements.length >= 150) break;
    const ref = elements.length + 1; el.setAttribute('data-cuckoo-ref', String(ref));
    const tag = el.tagName.toLowerCase(); const type = (el.getAttribute('type') || '').toLowerCase();
    const role = el.getAttribute('role') || (tag === 'a' ? 'link' : tag === 'select' ? 'select' : tag === 'textarea' ? 'textbox'
      : tag === 'input' ? (['checkbox', 'radio', 'submit', 'button', 'file'].includes(type) ? type : 'textbox') : tag);
    const label = el.labels && el.labels[0] ? el.labels[0].innerText : '';
    const name = (el.getAttribute('aria-label') || label || (tag === 'select' ? '' : el.innerText) || (type === 'password' || tag === 'select' ? '' : el.value) || el.getAttribute('placeholder')
      || el.getAttribute('title') || el.getAttribute('name') || '').trim().replace(/\\s+/g, ' ').slice(0, 80);
    const item = { ref, role, name };
    if (tag === 'a') item.href = el.href.slice(0, 200);
    if (type === 'password') item.password = true;
    else if (role === 'textbox' && el.value) item.value = String(el.value).slice(0, 80);
    if (tag === 'select') {
      item.options = [...el.options].slice(0, 30).map(o => o.text.trim().slice(0, 60));
      if (el.selectedIndex >= 0) item.value = el.options[el.selectedIndex].text.trim().slice(0, 60);
    }
    if (el.disabled) item.disabled = true;
    elements.push(item);
  }
  return { url: location.href, title: document.title, text: document.body ? document.body.innerText.slice(0, 12000) : '', elements };
})()`;

const find = ref => `document.querySelector('[data-cuckoo-ref="${Number(ref)}"]')`;

/** The element's centre after scrolling it into view, whether pressing it sends
 * something (a button, not a link or a field) and its name; or an error the agent can act on. */
async function centre(cdp, ref) {
  const box = await evaluate(cdp, `(() => { const el = ${find(ref)}; if (!el) return null;
    el.scrollIntoView({ block: 'center', inline: 'center' }); const r = el.getBoundingClientRect();
    const tag = el.tagName.toLowerCase(); const type = (el.getAttribute('type') || '').toLowerCase();
    const sends = tag === 'button' || el.getAttribute('role') === 'button' || (tag === 'input' && ['submit', 'button', 'image'].includes(type));
    const label = el.labels && el.labels[0] ? el.labels[0].innerText : '';
    const name = (el.getAttribute('aria-label') || label || (sends ? el.innerText || el.value : '') || el.getAttribute('placeholder')
      || el.getAttribute('name') || '').trim().replace(/\\s+/g, ' ').slice(0, 60);
    return { x: r.left + r.width / 2, y: r.top + r.height / 2, password: type === 'password', sends, name }; })()`);
  if (!box) throw new Error(`There is no element ${ref} on the page now. Read the page again.`);
  return box;
}

/** When pressing would send something and the owner's controls ask first, what it would
 * do, so Cuckoo can ask the person; nothing is pressed. */
const held = async (cdp, what) => ({ needs_submit: what, url: await evaluate(cdp, 'location.href') });

async function click(cdp, x, y) {
  for (const type of ['mouseMoved', 'mousePressed', 'mouseReleased']) {
    await cdp.call('Input.dispatchMouseEvent', { type, x, y, button: type === 'mouseMoved' ? 'none' : 'left', clickCount: 1 });
  }
}

const KEYS = { Enter: 13, Tab: 9, Escape: 27, Backspace: 8, ArrowDown: 40, ArrowUp: 38, ArrowLeft: 37, ArrowRight: 39, PageDown: 34, PageUp: 33, Home: 36, End: 35, Space: 32 };
async function press(cdp, key) {
  const code = KEYS[key];
  if (!code) throw new Error(`Press one of: ${Object.keys(KEYS).join(', ')}.`);
  const text = key === 'Enter' ? '\r' : key === 'Space' ? ' ' : undefined;
  await cdp.call('Input.dispatchKeyEvent', { type: 'keyDown', key: key === 'Space' ? ' ' : key, code: key, windowsVirtualKeyCode: code, ...(text ? { text } : {}) });
  await cdp.call('Input.dispatchKeyEvent', { type: 'keyUp', key: key === 'Space' ? ' ' : key, code: key, windowsVirtualKeyCode: code });
}

async function once(cdp) {
  switch (action) {
    case 'open': {
      const url = String(input.url || '');
      if (!/^https?:\/\//i.test(url)) throw new Error('Open an http or https address.');
      const result = await cdp.call('Page.navigate', { url });
      if (result.errorText) return { error: `The page could not be opened (${result.errorText}). The computer may not be allowed to reach this website.` };
      await settle(cdp);
      return evaluate(cdp, READ);
    }
    case 'read':
      return evaluate(cdp, READ);
    case 'click': {
      const { x, y, sends, name } = await centre(cdp, input.ref);
      if (sends && input.may_submit === false) return held(cdp, `press “${name || 'the button'}”`);
      await click(cdp, x, y);
      await settle(cdp, 10_000);
      return evaluate(cdp, READ);
    }
    case 'type': {
      const box = await centre(cdp, input.ref);
      if (box.password) throw new Error('Passwords are typed by a person. Ask for a hand-over instead.');
      if (input.submit && input.may_submit === false) return held(cdp, `send the form with “${box.name || 'this field'}”`);
      await click(cdp, box.x, box.y);
      await evaluate(cdp, `(() => { const el = ${find(input.ref)}; if ('value' in el) { el.value = ''; el.dispatchEvent(new Event('input', { bubbles: true })); } })()`);
      await cdp.call('Input.insertText', { text: String(input.text ?? '') });
      if (input.submit) { await press(cdp, 'Enter'); await settle(cdp, 10_000); }
      return evaluate(cdp, READ);
    }
    case 'choose': {
      const chosen = await evaluate(cdp, `(() => { const el = ${find(input.ref)}; if (!el || el.tagName !== 'SELECT') return false;
        const want = ${JSON.stringify(String(input.option ?? ''))}.trim().toLowerCase();
        const option = [...el.options].find(o => o.text.trim().toLowerCase() === want || o.value.toLowerCase() === want);
        if (!option) return false; el.value = option.value;
        el.dispatchEvent(new Event('input', { bubbles: true })); el.dispatchEvent(new Event('change', { bubbles: true })); return true; })()`);
      if (!chosen) throw new Error('Choose one of the options listed for that menu.');
      await settle(cdp, 5_000);
      return evaluate(cdp, READ);
    }
    case 'press':
      if (input.key === 'Enter' && input.may_submit === false) return held(cdp, 'press Enter');
      await press(cdp, String(input.key || ''));
      await settle(cdp, 10_000);
      return evaluate(cdp, READ);
    case 'screenshot': {
      // A small JPEG for Cuckoo itself (a hand-over's picture), or a PNG file for the agent.
      if (input.jpeg) {
        const shot = await cdp.call('Page.captureScreenshot', { format: 'jpeg', quality: 60 });
        return { jpeg: shot.data, url: await evaluate(cdp, 'location.href') };
      }
      const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
      const bytes = Buffer.from(shot.data, 'base64');
      mkdirSync(dirname(String(input.path)), { recursive: true });
      writeFileSync(String(input.path), bytes);
      return { path: input.path, bytes: bytes.length, url: await evaluate(cdp, 'location.href') };
    }
    default:
      throw new Error(`Unknown browser action ${action}.`);
  }
}

/** The live picture for a person: frames out, their mouse and keys in. */
async function live(cdp) {
  const write = line => process.stdout.write(`${JSON.stringify(line)}\n`);
  cdp.listeners.push(message => {
    if (message.method === 'Page.screencastFrame') {
      const { data, metadata, sessionId } = message.params;
      write({ frame: data, width: metadata.deviceWidth, height: metadata.deviceHeight });
      cdp.call('Page.screencastFrameAck', { sessionId }).catch(() => {});
    } else if (message.method === 'Page.frameNavigated' && !message.params.frame.parentId) {
      write({ url: message.params.frame.url });
    }
  });
  await cdp.call('Page.startScreencast', { format: 'jpeg', quality: 60, maxWidth: 1280, maxHeight: 800, everyNthFrame: 1 });
  write({ url: await evaluate(cdp, 'location.href'), ready: true });
  const lines = createInterface({ input: process.stdin });
  for await (const line of lines) {
    try {
      const event = JSON.parse(line);
      if (event.stop) break;
      if (event.mouse) await cdp.call('Input.dispatchMouseEvent', { button: 'left', clickCount: 1, ...event.mouse });
      if (event.wheel) await cdp.call('Input.dispatchMouseEvent', { type: 'mouseWheel', x: event.wheel.x, y: event.wheel.y, deltaX: 0, deltaY: event.wheel.deltaY });
      if (typeof event.text === 'string') await cdp.call('Input.insertText', { text: event.text });
      if (event.key) await press(cdp, event.key);
    } catch (error) { write({ problem: String(error.message || error) }); }
  }
  await cdp.call('Page.stopScreencast').catch(() => {});
}

try {
  const cdp = await connect();
  if (action === 'live') { await live(cdp); cdp.close(); process.exit(0); }
  const result = await once(cdp);
  cdp.close();
  done(result);
} catch (error) {
  fail(String(error.message || error));
}
