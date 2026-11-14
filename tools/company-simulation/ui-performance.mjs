// Chat speed baseline: in-page timings and long tasks for opening Chat, switching conversations
// and opening a 200-message channel, plus the production bundle size. Compares with the committed
// baseline and warns on regressions (exit 1 only with --strict).
// Usage: node tools/company-simulation/ui-performance.mjs [runs-dir] [--repeat 5] [--cpu-throttle 1]
//   [--strict] [--update-baseline] [--baseline path]
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { cpus } from 'node:os';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { api, fillConversation, latestRun, launch, namedChannel, parseArguments, signedIn, workspaceUrl } from './browser-session.mjs';
import { bundleSize, compare, defaultThresholds, formatRows, metrics } from './ui-performance-baseline.mjs';

const { runs, options } = parseArguments(process.argv.slice(2), ['repeat', 'cpu-throttle', 'baseline']);
const repeat = Number(options.repeat ?? 5), throttle = Number(options['cpu-throttle'] ?? 1);
if (!(repeat >= 1) || !(throttle >= 1)) throw new Error('--repeat and --cpu-throttle must be numbers of at least 1');
const baselinePath = options.baseline ?? fileURLToPath(new URL('./ui-performance-baseline.json', import.meta.url));
const viewport = { width: 1366, height: 900 };
const { directory, c } = await latestRun(runs);
const bundle = await bundleSize();
const browser = await launch();

// Fixtures, reused by name across runs so every run measures the same conversations. The
// measuring employee writes them, so no unread divider or saved reading position moves the view.
const owner = await signedIn(browser, c, 0, { viewport }), request = api(owner, c);
const people = [c.employees[0].user.id, c.employees[1].user.id];
const topics = ['Stock count for **office desks** is 12 of 20; 8 arrive Thursday.', 'Finance needs the supplier quote before approving the Northstar price.',
  'Driver confirmed the delivery window: 9–11 am, loading bay B.', 'Reminder: update the order tracker after each call.\n\n- quote sent\n- deposit pending'];
async function fixture(name, tag, count) {
  const id = await namedChannel(request, c, name, people);
  await fillConversation([request], c, id, count, i => `${topics[i % topics.length]} [speed-${tag}-${String(i + 1).padStart(3, '0')}]`);
  return { id, marker: `[speed-${tag}-`, count };
}
const small = [await fixture('Speed check · channel A', 'A', 40), await fixture('Speed check · channel B', 'B', 40)];
const large = await fixture('Speed check · 200 messages', 'L', 200);

const page = await owner.newPage(), errors = [];
page.on('pageerror', error => errors.push(String(error)));
await owner.addInitScript(() => {
  window.__uiLongTasks = [];
  new PerformanceObserver(list => { for (const entry of list.getEntries()) window.__uiLongTasks.push([entry.startTime, entry.duration]); }).observe({ type: 'longtask', buffered: true });
});
if (throttle > 1) await (await owner.newCDPSession(page)).send('Emulation.setCPUThrottlingRate', { rate: throttle });

// Predicates run every animation frame and return the frame's timestamp once true. A conversation
// counts as shown once any of its marked messages is inside the viewport.
const listShown = () => {
  const link = document.querySelector('aside[aria-label="Conversations"] a[href*="conversation="]');
  return link && link.checkVisibility() && link.getBoundingClientRect().height > 0 ? performance.now() : false;
};
const messageShown = marker => {
  const nodes = document.evaluate(`//article[contains(., "${marker}")]`, document, null, XPathResult.ORDERED_NODE_SNAPSHOT_TYPE, null);
  for (let i = nodes.snapshotLength - 1; i >= 0; i--) {
    const rect = nodes.snapshotItem(i).getBoundingClientRect();
    if (rect.height > 0 && rect.bottom > 0 && rect.top < innerHeight) return performance.now();
  }
  return false;
};
const until = async (predicate, arg) => (await page.waitForFunction(predicate, arg, { polling: 'raf', timeout: 30000 })).jsonValue();
const quiet = async () => { await page.waitForTimeout(600); await page.evaluate(() => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)))); };
const longTasks = async (from, to) => { await page.waitForTimeout(100); return page.evaluate(([from, to]) => window.__uiLongTasks.filter(([start]) => start >= from && start <= to).length, [from, to]); };
// Clicks inside the page so the start time and the click share one clock and one task.
const clickAt = selector => page.evaluate(selector => {
  const target = document.querySelector(selector);
  if (!target) throw new Error(`Nothing matches ${selector}`);
  const start = performance.now(); target.click(); return start;
}, selector);
const contact = id => `aside[aria-label="Conversations"] a[href$="conversation=${id}"]`;

const samples = Object.fromEntries(metrics.filter(m => !m.key.startsWith('bundle')).map(m => [m.key, []]));
let renderedMessages = 0;
async function journey(record) {
  const keep = (key, value) => { if (record) samples[key].push(value); };
  await page.goto(workspaceUrl(c, 'view=chat'), { waitUntil: 'commit' });
  const shown = await until(listShown);
  keep('chat_open_cold_ms', shown); keep('chat_open_cold_long_tasks', await longTasks(0, shown));

  await page.goto(workspaceUrl(c, 'view=home'));
  await page.getByRole('main', { name: 'Home' }).waitFor(); await quiet();
  let start = await clickAt('nav[aria-label="Workspace"] a[href*="view=chat"]'), end = await until(listShown);
  keep('chat_open_from_home_ms', end - start); keep('chat_open_from_home_long_tasks', await longTasks(start, end));

  await page.goto(workspaceUrl(c, `view=chat&conversation=${small[0].id}`));
  await until(messageShown, small[0].marker); await quiet();
  for (const to of [small[1], small[0]]) {
    start = await clickAt(contact(to.id)); end = await until(messageShown, to.marker);
    keep('conversation_switch_ms', end - start); keep('conversation_switch_long_tasks', await longTasks(start, end));
    await quiet();
  }
  start = await clickAt(contact(large.id)); end = await until(messageShown, large.marker);
  keep('large_conversation_ms', end - start); keep('large_conversation_long_tasks', await longTasks(start, end));
  renderedMessages = await page.locator('article[aria-label^="Message from"]').count();
}
await journey(false); // Warm-up: server caches, HTTP cache and read receipts.
for (let i = 0; i < repeat; i++) { await journey(true); console.log(`Repetition ${i + 1}/${repeat} done`); }
const chromium = browser.version();
await browser.close();

const median = values => { const sorted = [...values].sort((a, b) => a - b), middle = sorted.length >> 1; return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2; };
const current = Object.fromEntries(Object.entries(samples).map(([key, values]) => [key, Math.round(median(values) * 10) / 10]));
Object.assign(current, { bundle_entry_gzip_kB: bundle.bundle_entry_gzip_kB, bundle_total_gzip_kB: bundle.bundle_total_gzip_kB });
const environment = { chromium, cpus: cpus().length, cpu_model: cpus()[0]?.model, node: process.version, cpu_throttle: throttle, viewport, repeat,
  source_head: execFileSync('git', ['rev-parse', '--short', 'HEAD'], { encoding: 'utf8' }).trim(), server: 'debug build, local simulation' };
const fixtures = { switch_messages: small[0].count, large_messages: large.count, large_rendered_articles: renderedMessages };
const run = { created_at: new Date().toISOString(), environment, fixtures, metrics: current, samples, entry_assets: bundle.entry_assets, errors };

let baseline;
try { baseline = JSON.parse(await readFile(baselinePath, 'utf8')); } catch { baseline = undefined; }
const rows = baseline ? compare(current, baseline) : [];
const regressions = rows.filter(row => row.regressed);
await writeFile(join(directory, `ui-performance-${run.created_at.replaceAll(':', '-')}.json`), JSON.stringify({ ...run, comparison: rows }, null, 2));
if (baseline) {
  console.log(`\nCompared with the baseline from ${baseline.created_at} (${baseline.environment?.source_head ?? 'unknown commit'}):\n${formatRows(rows)}`);
  if (baseline.environment?.cpu_throttle !== throttle) console.log(`Note: the baseline used CPU throttle ${baseline.environment?.cpu_throttle}; this run used ${throttle}.`);
} else console.log(`\nNo baseline at ${baselinePath}; run with --update-baseline to create one.\n${JSON.stringify(current, null, 2)}`);
console.log(`${rows.length ? `${regressions.length} of ${rows.length} metrics regressed beyond ${Math.round(defaultThresholds.ratio * 100)}% and the absolute floor.` : ''} Rendered ${renderedMessages} message articles in the large channel; ${errors.length} page errors.`);
if (options['update-baseline']) {
  await writeFile(baselinePath, JSON.stringify({ created_at: run.created_at, thresholds: baseline?.thresholds ?? defaultThresholds, environment, fixtures, metrics: current, entry_assets: bundle.entry_assets }, null, 2) + '\n');
  console.log(`Baseline written to ${baselinePath}`);
}
process.exitCode = options.strict && (regressions.length || errors.length) ? 1 : 0;
