/** What the live stream tells people after a gap, beside the connection status:
 * how much it caught up after reconnecting, that the gap was longer than the
 * server keeps (so screens reloaded), or that a newer Cuckoo is out. */
export type LiveNotice =
  | { kind: 'back'; messages: number }
  | { kind: 'reloaded' }
  | { kind: 'updated' };

let notice: LiveNotice | undefined;
const subscribers = new Set<() => void>();
export const liveNotice = () => notice;
export function subscribeNotice(fn: () => void) { subscribers.add(fn); return () => { subscribers.delete(fn); }; }
function publish(next: LiveNotice | undefined) { notice = next; subscribers.forEach(fn => fn()); }

/** A newer build stays announced: only a reload clears it. */
export function showNotice(next: LiveNotice) { if (notice?.kind !== 'updated' || next.kind === 'updated') publish(next); }
export function clearNotice(kind: LiveNotice['kind']) { if (notice?.kind === kind) publish(undefined); }

/** The entry script this tab was built from, or nothing on the development server. */
export function ownBuild(doc: Document = document) {
  const src = doc.querySelector('script[type="module"][src^="/assets/"]')?.getAttribute('src');
  return src ?? undefined;
}
/** Whether the server hands out a different build than this tab runs. */
export function outdated(serverBuild: unknown, mine = ownBuild()) {
  return typeof serverBuild === 'string' && Boolean(mine) && serverBuild !== mine;
}
