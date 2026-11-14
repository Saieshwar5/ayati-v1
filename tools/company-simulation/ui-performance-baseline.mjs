// Production bundle size and comparison of UI speed measurements with the committed baseline.
import { readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { gzipSync } from 'node:zlib';

// A metric regresses only when it is both `ratio` worse and worse by more than its unit's floor,
// so small absolute jitter on fast interactions does not raise warnings.
export const defaultThresholds = { ratio: 0.2, floor: { ms: 50, tasks: 2, kB: 5 } };

export const metrics = [
  { key: 'chat_open_cold_ms', unit: 'ms', label: 'Load Chat (full page) until the conversation list shows' },
  { key: 'chat_open_from_home_ms', unit: 'ms', label: 'Home → Chat in the app until the conversation list shows' },
  { key: 'conversation_switch_ms', unit: 'ms', label: 'Switch between 40-message channels until the transcript shows' },
  { key: 'large_conversation_ms', unit: 'ms', label: 'Open a 200-message channel until its transcript shows' },
  { key: 'chat_open_cold_long_tasks', unit: 'tasks', label: 'Long tasks (>50 ms) while loading Chat' },
  { key: 'chat_open_from_home_long_tasks', unit: 'tasks', label: 'Long tasks while going Home → Chat' },
  { key: 'conversation_switch_long_tasks', unit: 'tasks', label: 'Long tasks while switching conversations' },
  { key: 'large_conversation_long_tasks', unit: 'tasks', label: 'Long tasks while opening the 200-message channel' },
  { key: 'bundle_entry_gzip_kB', unit: 'kB', label: 'Gzipped JS/CSS that web/dist/index.html loads at start' },
  { key: 'bundle_total_gzip_kB', unit: 'kB', label: 'Gzipped JS/CSS in web/dist/assets, including lazy chunks' },
];

const gzipKilobytes = bytes => gzipSync(bytes, { level: 9 }).length / 1000;

/** Gzip sizes of the built client: the assets index.html references, and every JS/CSS asset. */
export async function bundleSize(dist = 'web/dist') {
  const html = await readFile(join(dist, 'index.html'), 'utf8');
  const entry = [...new Set([...html.matchAll(/(?:src|href)="\/(assets\/[^"]+\.(?:js|css))"/g)].map(match => match[1]))];
  const all = (await readdir(join(dist, 'assets'))).filter(name => /\.(js|css)$/.test(name)).map(name => `assets/${name}`);
  const sizes = Object.fromEntries(await Promise.all(all.map(async path => [path, gzipKilobytes(await readFile(join(dist, path)))])));
  const sum = paths => Math.round(paths.reduce((total, path) => total + (sizes[path] ?? 0), 0) * 10) / 10;
  if (!entry.length) throw new Error(`No entry assets found in ${dist}/index.html; run npm run build --prefix web`);
  return { entry_assets: entry, bundle_entry_gzip_kB: sum(entry), bundle_total_gzip_kB: sum(all) };
}

/** One row per metric present in both runs; `regressed` follows the thresholds above. */
export function compare(current, baseline) {
  const thresholds = { ...defaultThresholds, ...baseline.thresholds, floor: { ...defaultThresholds.floor, ...baseline.thresholds?.floor } };
  return metrics.filter(m => typeof current[m.key] === 'number' && typeof baseline.metrics?.[m.key] === 'number').map(m => {
    const before = baseline.metrics[m.key], now = current[m.key], change = now - before;
    const regressed = now > before * (1 + thresholds.ratio) && change > thresholds.floor[m.unit];
    return { ...m, baseline: before, current: now, change, change_percent: before ? Math.round(change / before * 1000) / 10 : null, regressed };
  });
}

export function formatRows(rows) {
  const value = (number, unit) => unit === 'ms' ? `${Math.round(number)} ms` : unit === 'kB' ? `${number.toFixed(1)} kB` : `${number}`;
  return rows.map(r => `${r.regressed ? 'WARN' : 'ok  '} ${r.label}: ${value(r.current, r.unit)} (baseline ${value(r.baseline, r.unit)}${r.change_percent === null ? '' : `, ${r.change >= 0 ? '+' : ''}${r.change_percent}%`})`).join('\n');
}
