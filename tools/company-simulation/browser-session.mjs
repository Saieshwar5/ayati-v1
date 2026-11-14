// Shared browser launch, simulated sign-in and API access for the layout and speed checks.
// Like the other simulation scripts it reads the latest run manifest written by run.mjs.
import { chromium } from '../../web/node_modules/playwright-core/index.mjs';
import { readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';

// The other scripts use the distribution's Chromium; AYATI_CHROMIUM points at another build.
export const chromiumPath = process.env.AYATI_CHROMIUM ?? '/usr/bin/chromium';
const headers = { Origin: '', 'X-Ayati-Request': 'identity' };

/** Positional runs directory plus `--name value` / `--flag` options. */
export function parseArguments(argv, valued) {
  const options = {}, positional = [];
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (!arg.startsWith('--')) { positional.push(arg); continue; }
    const name = arg.slice(2);
    if (valued.includes(name)) {
      if (argv[i + 1] === undefined) throw new Error(`--${name} needs a value`);
      options[name] = argv[++i];
    } else options[name] = true;
  }
  return { runs: positional[0] ?? '.ayati-local/company-simulation/runs', options };
}

export async function latestRun(runs) {
  const { directory } = JSON.parse(await readFile(resolve(runs, 'latest.json')));
  const manifest = JSON.parse(await readFile(join(directory, 'manifest.json')));
  if (new URL(manifest.base).hostname !== '127.0.0.1') throw new Error('Only loopback simulation instances are supported');
  return { directory, c: manifest };
}

export const launch = () => chromium.launch({ headless: true, executablePath: chromiumPath, args: ['--no-sandbox'] });

/** A browser context signed in as one fictional employee through the simulation-only login. */
export async function signedIn(browser, c, employee, options = {}) {
  const context = await browser.newContext(options);
  const response = await context.request.post(`${c.base}/api/identity/simulation/employees/${employee}/login`, { headers: { ...headers, Origin: c.base }, data: {} });
  if (!response.ok()) throw new Error(`Simulation sign-in for employee ${employee} failed: HTTP ${response.status()}`);
  return context;
}

/** JSON requests with the context's session cookie: GET without a body, POST with one. */
export function api(context, c) {
  return async (path, body) => {
    const options = { headers: { ...headers, Origin: c.base }, timeout: 30000 };
    const response = body === undefined ? await context.request.get(c.base + path, options) : await context.request.post(c.base + path, { ...options, data: body });
    if (!response.ok()) throw new Error(`${body === undefined ? 'GET' : 'POST'} ${path.replace(/[0-9a-f-]{36}/g, ':id')}: HTTP ${response.status()} ${(await response.text()).slice(0, 300)}`);
    return response.json();
  };
}

export const workspaceUrl = (c, query) => `${c.base}/w/${c.w.split('/').at(-1)}/?${query}`;

/** Find a non-archived channel by exact name, or create it with the given members. */
export async function namedChannel(request, c, name, users, agents = []) {
  const { chats } = await request(`${c.w}/chats`);
  const existing = chats.find(chat => chat.name === name && chat.kind === 'channel' && !chat.archived);
  if (existing) return existing.id;
  const id = randomUUID();
  await request(`${c.w}/chats`, { id, name, kind: 'channel', description: 'Fixture for the layout and speed checks', users, agents });
  return id;
}

/** Ensure a conversation holds at least `count` top-level messages, writing only the missing ones.
 * `requests` are signed-in members who take turns as authors; the first one reads the history. */
export async function fillConversation(requests, c, chat, count, content) {
  const { messages } = await requests[0](`${c.w}/chats/${chat}/messages`);
  const present = messages.filter(message => !message.thread).length;
  for (let i = present; i < count; i++) await requests[i % requests.length](`${c.w}/chats/${chat}/messages`, { id: randomUUID(), content: content(i) });
  return Math.max(present, count);
}
