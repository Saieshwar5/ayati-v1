// Phone and tablet layout audit of the main workspace screens, light and dark.
// Usage: node tools/company-simulation/mobile-layout.mjs [runs-dir] [--sizes 320x568,390x844]
//   [--screens home,channel] [--themes light] [--no-screenshots]
// Exits 1 when any rule fails; see runtime/layout-and-speed-checks.md.
import { mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { randomUUID } from 'node:crypto';
import { api, fillConversation, latestRun, launch, namedChannel, parseArguments, signedIn, workspaceUrl } from './browser-session.mjs';
import { auditLayout } from './layout-rules.mjs';

const sizes = [
  { name: 'small phone', width: 320, height: 568 },
  { name: 'Android compact', width: 360, height: 740 },
  { name: 'iPhone SE', width: 375, height: 667 },
  { name: 'iPhone 14', width: 390, height: 844 },
  { name: 'iPhone 15', width: 393, height: 852 },
  { name: 'Android large', width: 412, height: 915 },
  { name: 'iPhone Pro Max', width: 430, height: 932 },
  { name: 'iPad mini', width: 768, height: 1024 },
  { name: 'iPad Air', width: 820, height: 1180 },
];
const MIN_TARGET = 44, TOLERANCE = 1;
const { runs, options } = parseArguments(process.argv.slice(2), ['sizes', 'screens', 'themes']);
const { directory, c } = await latestRun(runs);
const output = join(directory, `mobile-layout-${new Date().toISOString().replaceAll(':', '-')}`);
await mkdir(output, { recursive: true, mode: 0o700 });
const browser = await launch();

// Fixtures: a channel with a thread and a direct conversation with the owner's team agent.
const owner = await signedIn(browser, c, 0), request = api(owner, c);
const team = c.employees.filter(e => e.team === c.employees[0].team);
const channel = await namedChannel(request, c, 'Layout audit · phone review', team.map(e => e.user.id), [c.agents[0].id]);
const sample = i => [
  'Morning check: **delivery van 2** is booked for Northstar Studio; the invoice reference is NS-2026-0042-OFFICE-DESKS.',
  'Long link test https://example.com/orders/northstar-studio/2026/11/delivery-confirmation-with-a-very-long-path-segment',
  '| Item | Ordered | Available |\n|---|---|---|\n| Office desk | 20 | 12 |\n| Chair | 40 | 40 |',
  'Can someone confirm the stock count before Finance approves the price? Supercalifragilisticexpialidociousreferencecode',
][i % 4];
await fillConversation([request], c, channel, 8, sample);
const { messages } = await request(`${c.w}/chats/${channel}/messages`);
let root = messages.find(m => !m.thread && messages.some(reply => reply.thread === m.id));
if (!root) {
  root = messages.find(m => !m.thread);
  await request(`${c.w}/chats/${channel}/messages`, { id: randomUUID(), thread: root.id, content: 'Thread reply: stock count confirmed at 12 desks; 8 arrive Thursday.' });
}
const direct = (await request(`${c.w}/chats`, { id: randomUUID(), kind: 'direct', name: c.agents[0].name, description: '', users: [], agents: [c.agents[0].id] })).id;
const directHistory = await request(`${c.w}/chats/${direct}/messages`);
if (!directHistory.messages.length) await request(`${c.w}/chats/${direct}/messages`, { id: randomUUID(), content: 'Summarise today’s open orders, please.', agent: c.agents[0].id });
await owner.close();

const message = page => page.getByRole('textbox', { name: 'Message', exact: true });
const screens = [
  { id: 'home', query: 'view=home', ready: page => page.getByRole('main', { name: 'Home' }) },
  { id: 'chat-list', query: 'view=chat', ready: page => page.getByRole('complementary', { name: 'Conversations' }).getByRole('link').first() },
  { id: 'channel', query: `view=chat&conversation=${channel}`, ready: message },
  { id: 'thread', query: `view=chat&conversation=${channel}`, ready: message, scope: 'dialog[open]', open: async page => {
    await page.getByRole('button', { name: /^\d+ repl(y|ies)$/ }).first().tap();
    await page.getByRole('dialog', { name: 'Thread' }).getByRole('textbox', { name: 'Reply in thread' }).waitFor();
  } },
  { id: 'agent-direct', query: `view=chat&conversation=${direct}`, ready: message },
  { id: 'agents', query: 'view=agents', ready: page => page.getByRole('heading', { name: /^Your team/ }) },
  { id: 'agent-profile', query: `view=agents&selected=${c.agents[0].id}`, ready: page => page.getByRole('heading', { name: c.agents[0].name, exact: true }) },
  { id: 'activity', query: 'view=activity', ready: page => page.getByRole('heading', { name: 'Activity', exact: true }) },
  { id: 'apps', query: 'view=apps', ready: page => page.getByRole('heading', { name: 'Apps', exact: true }) },
  { id: 'library', query: 'view=library', ready: page => page.getByRole('heading', { name: 'Library', exact: true }) },
  { id: 'settings', query: 'view=settings', ready: page => page.getByRole('main', { name: 'Settings' }) },
  { id: 'settings-profile', query: 'view=settings&section=profile', ready: page => page.getByRole('main', { name: 'Settings' }) },
  { id: 'settings-people', query: 'view=settings&section=people', ready: page => page.getByRole('main', { name: 'Settings' }) },
];
const pick = (all, list, key) => list ? all.filter(item => list.split(',').includes(key(item))) : all;
const chosenSizes = pick(sizes, options.sizes, s => `${s.width}x${s.height}`);
const chosenScreens = pick(screens, options.screens, s => s.id);
const themes = options.themes ? options.themes.split(',') : ['light', 'dark'];
if (!chosenSizes.length || !chosenScreens.length) throw new Error('No sizes or screens match the given filters');

// Loading placeholders end in an ellipsis ("Opening Apps…"); wait for them to go, then two frames.
async function settle(page) {
  await page.waitForFunction(() => ![...document.querySelectorAll('[role=status]')].some(node => /…$/.test(node.textContent.trim()) && node.checkVisibility()), null, { timeout: 8000 }).catch(() => {});
  await page.evaluate(() => document.fonts.ready.then(() => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)))));
  await page.waitForTimeout(250);
}

const results = [];
for (const theme of themes) for (const size of chosenSizes) {
  const context = await signedIn(browser, c, 0, { viewport: { width: size.width, height: size.height }, deviceScaleFactor: 2, isMobile: true, hasTouch: true, colorScheme: theme });
  await context.addInitScript(value => { try { localStorage.setItem('ayati.theme', value); } catch { /* storage blocked */ } }, theme);
  const page = await context.newPage(), errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  for (const screen of chosenScreens) {
    const result = { screen: screen.id, size: `${size.width}x${size.height}`, device: size.name, theme, findings: [], errors: [] };
    errors.length = 0;
    try {
      await page.goto(workspaceUrl(c, screen.query));
      await screen.ready(page).waitFor({ timeout: 15000 });
      if (screen.open) await screen.open(page);
      await settle(page);
      Object.assign(result, await page.evaluate(auditLayout, { minTarget: MIN_TARGET, tolerance: TOLERANCE, scope: screen.scope }));
    } catch (error) {
      result.findings.push({ rule: 'screen-unavailable', selector: '', name: '', area: '', detail: String(error).split('\n')[0] });
    }
    result.errors = [...errors];
    if (result.findings.length && !options['no-screenshots']) {
      // Outline offenders through the CSSOM; the app's content security policy blocks injected <style> tags.
      await page.evaluate(() => document.querySelectorAll('[data-layout-audit]').forEach(el => el.style.setProperty('outline', '2px solid #e5484d', 'important'))).catch(() => {});
      result.screenshot = join(output, `${screen.id}-${result.size}-${theme}.png`);
      await page.screenshot({ path: result.screenshot }).catch(() => { delete result.screenshot; });
    }
    results.push(result);
    console.log(`${result.findings.length ? 'FAIL' : 'PASS'} ${screen.id} ${result.size} ${theme}${result.findings.length ? ` (${result.findings.length})` : ''}`);
  }
  await context.close();
}
await browser.close();

// Group identical problems so one repeated control reads as one line with a count.
const groups = new Map();
for (const result of results) for (const finding of result.findings) {
  const key = [finding.rule, result.screen, finding.selector, finding.area].join('|');
  const group = groups.get(key) ?? { rule: finding.rule, screen: result.screen, selector: finding.selector, area: finding.area, sizes: new Set(), themes: new Set(), names: new Set(), detail: finding.detail, count: 0 };
  group.sizes.add(result.size); group.themes.add(result.theme); if (finding.name) group.names.add(finding.name); group.count++;
  groups.set(key, group);
}
const summary = [...groups.values()].map(g => ({ ...g, sizes: [...g.sizes], themes: [...g.themes], names: [...g.names].slice(0, 4) }))
  .sort((a, b) => a.rule.localeCompare(b.rule) || a.screen.localeCompare(b.screen) || b.count - a.count);
const failed = results.filter(r => r.findings.length);
const report = { created_at: new Date().toISOString(), base: c.base, rules: { min_target_px: MIN_TARGET, tolerance_px: TOLERANCE }, sizes: chosenSizes, themes, screens: chosenScreens.map(s => s.id),
  totals: { checks: results.length, failed: failed.length, findings: results.reduce((n, r) => n + r.findings.length, 0), page_errors: results.reduce((n, r) => n + r.errors.length, 0) }, summary, results };
await writeFile(join(output, 'report.json'), JSON.stringify(report, null, 2));

console.log(`\n${failed.length}/${results.length} screen checks failed; ${report.totals.findings} findings, ${report.totals.page_errors} page errors.`);
for (const g of summary) console.log(`- ${g.rule} · ${g.screen} · ${g.selector}${g.area ? ` in ${g.area}` : ''} · ${g.count}× at ${g.sizes.join(', ')} (${g.themes.join('/')})${g.names.length ? ` · e.g. "${g.names.join('", "')}"` : ''} · ${g.detail}`);
console.log(`Report: ${join(output, 'report.json')}`);
process.exitCode = failed.length ? 1 : 0;
