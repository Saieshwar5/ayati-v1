import type { ChartSection, ChartVariant, Point } from '../../panel-types';

/** A read-only document inside an agent's reply, between `<page title="…">` and
 * `</page>` lines. `src/response_page.rs` reads the same format on the server. */
export type ReplyPage = { title: string; body: string; complete: boolean };
/** What the card in Chat says about a page, without its text. */
export type PageSummary = { title: string; sections: number; minutes: number; blocks: string[]; complete: boolean };
export type PageStep = { title: string; detail: string };
export type PageBlock =
  | { kind: 'markdown'; text: string }
  | { kind: 'chart'; section: ChartSection }
  | { kind: 'steps'; steps: PageStep[] }
  | { kind: 'unreadable'; label: string; text: string };

const maxTitle = 120, wordsPerMinute = 200, maxSteps = 8;
const opening = /^<page title="([^"<>]+)">$/;

/** Splits a reply into its words and its first page. Later pages stay as text. */
export function splitPage(text: string): { text: string; page?: ReplyPage } {
  const lines = text.split('\n');
  const start = lines.findIndex(line => {
    const title = opening.exec(line.trimEnd())?.[1];
    return Boolean(title?.trim()) && [...title!].length <= maxTitle;
  });
  if (start < 0) return { text };
  const title = opening.exec(lines[start].trimEnd())![1];
  const close = lines.findIndex((line, index) => index > start && line.trim() === '</page>');
  const end = close < 0 ? lines.length : close;
  const before = lines.slice(0, start).join('\n').trimEnd();
  const after = lines.slice(end + 1).join('\n').trim();
  return { text: before && after ? `${before}\n\n${after}` : before || after,
    page: { title, body: lines.slice(start + 1, end).join('\n').replace(/^\n+|\n+$/g, ''), complete: close >= 0 } };
}

const fence = (line: string) => line.trimEnd().startsWith('```') ? line.trimEnd().slice(3).trim() : undefined;

export function pageSummary(page: ReplyPage): PageSummary {
  let sections = 0, words = 0, fenced = false;
  const blocks: string[] = [];
  const has = (kind: string) => { if (!blocks.includes(kind)) blocks.push(kind); };
  for (const raw of page.body.split('\n')) {
    const line = raw.trimEnd(), kind = fence(line);
    if (kind !== undefined) {
      fenced = !fenced;
      if (fenced && kind === 'chart') has('chart');
      if (fenced && kind === 'steps') has('diagram');
      continue;
    }
    if (fenced) continue;
    if (line.startsWith('## ')) sections++; else if (line.startsWith('|')) has('table');
    words += line.split(/\s+/).filter(Boolean).length;
  }
  return { title: page.title, sections, blocks, complete: page.complete, minutes: Math.max(1, Math.ceil(words / wordsPerMinute)) };
}

/** Section headings (`## `) outside fenced blocks, in order. */
export function pageOutline(body: string): string[] {
  let fenced = false;
  return body.split('\n').flatMap(line => {
    if (fence(line) !== undefined) { fenced = !fenced; return []; }
    return !fenced && line.startsWith('## ') ? [line.slice(3).replace(/[*_`]/g, '').trim()] : [];
  });
}

const variants: ChartVariant[] = ['line', 'bar', 'area', 'stacked_bar', 'donut'];
const text = (value: unknown, max: number): value is string => typeof value === 'string' && value.length > 0 && value.length <= max;
function points(value: unknown): Point[] | undefined {
  if (!Array.isArray(value) || value.length < 1 || value.length > 12) return;
  const result = value.map(item => item && text(item.label, 40) && Number.isFinite(item.value) ? { label: item.label as string, value: item.value as number } : undefined);
  return result.every(Boolean) ? result as Point[] : undefined;
}
/** The Home panel chart contract, checked again because page text is model output. */
export function chartSection(source: string): ChartSection | undefined {
  let data: Record<string, unknown>;
  try { data = JSON.parse(source); } catch { return; }
  if (!data || typeof data !== 'object' || !text(data.title, 100)) return;
  if (data.unit !== undefined && !text(data.unit, 40)) return;
  const variant = (data.variant ?? 'bar') as ChartVariant;
  if (!variants.includes(variant)) return;
  const base = { kind: 'chart' as const, title: data.title, variant, ...(data.unit ? { unit: data.unit as string } : {}) };
  let series: { name: string; points: Point[] }[];
  if (data.series !== undefined) {
    if (data.points !== undefined || !Array.isArray(data.series) || data.series.length < 1 || data.series.length > 4) return;
    const parsed = data.series.map(item => ({ name: item?.name, points: points(item?.points) }));
    if (!parsed.every(item => text(item.name, 60) && item.points)) return;
    series = parsed as typeof series;
    const labels = series[0].points.map(point => point.label).join('\n');
    if (series.some(item => item.points.map(point => point.label).join('\n') !== labels)) return;
    if (new Set(series.map(item => item.name)).size !== series.length) return;
  } else {
    const single = points(data.points);
    if (!single) return;
    series = [{ name: data.title, points: single }];
  }
  const negative = series.some(item => item.points.some(point => point.value < 0));
  if (negative && (variant === 'donut' || variant === 'stacked_bar')) return;
  if (variant === 'donut' && series.length > 1) return;
  return data.series !== undefined ? { ...base, series } : { ...base, points: series[0].points };
}

function steps(source: string): PageStep[] | undefined {
  const result = source.split('\n').map(line => line.trim()).filter(Boolean).map(line => {
    const [title, ...detail] = line.split('|');
    return { title: title.trim(), detail: detail.join('|').trim() };
  });
  return result.length >= 2 && result.length <= maxSteps && result.every(step => step.title && step.title.length <= 60 && step.detail.length <= 120) ? result : undefined;
}

/** A page body as Markdown runs, charts and step diagrams. A block that does not
 * fit its contract is shown as its text, never guessed at. */
export function pageBlocks(body: string): PageBlock[] {
  const blocks: PageBlock[] = [];
  let markdown: string[] = [], special: { kind: string; lines: string[] } | undefined, fenced = false;
  const flush = () => { if (markdown.join('').trim()) blocks.push({ kind: 'markdown', text: markdown.join('\n') }); markdown = []; };
  for (const line of body.split('\n')) {
    const kind = fence(line);
    if (special) {
      if (kind === undefined) { special.lines.push(line); continue; }
      const source = special.lines.join('\n');
      const chart = special.kind === 'chart' ? chartSection(source) : undefined;
      const diagram = special.kind === 'steps' ? steps(source) : undefined;
      blocks.push(chart ? { kind: 'chart', section: chart } : diagram ? { kind: 'steps', steps: diagram }
        : { kind: 'unreadable', label: special.kind === 'chart' ? 'This chart could not be drawn.' : 'This diagram could not be drawn.', text: source });
      special = undefined;
    } else if (kind !== undefined && !fenced && (kind === 'chart' || kind === 'steps')) {
      flush(); special = { kind, lines: [] };
    } else {
      if (kind !== undefined) fenced = !fenced;
      markdown.push(line);
    }
  }
  // An unfinished block at the end is a page still being written: keep its text.
  if (special) markdown.push('```', ...special.lines);
  flush();
  return blocks;
}
