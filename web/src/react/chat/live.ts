import { workspaceId, workspaceSession, storageScope } from '../../workspace-context';
import { chatRequest, type Message } from './model';
import { flushOutbox, stopOutbox, subscribeSent } from './outbox';
import { applyPresence } from './presence';
import { outdated, showNotice } from './live-notice';
export type Change = { kind: 'message' | 'access' | 'reset' | 'sample_access' | 'event_access' | 'panel_access' | 'agents'; source?: string; chat?: string; message?: Message };
const subscribers = new Set<(change: Change) => void>();
const statusSubscribers = new Set<() => void>();
let status = 'Connecting…';
export const liveStatus = () => status;
export function subscribeStatus(fn: () => void) { statusSubscribers.add(fn); return () => { statusSubscribers.delete(fn); }; }
export function subscribeChanges(fn: (change: Change) => void) { subscribers.add(fn); return () => { subscribers.delete(fn); }; }
const setStatus = (value: string) => { status = value; statusSubscribers.forEach(fn => fn()); };
/** The connection shows only while it is down (D245, screen 2 note 5): offline, or trying again. */
export function connectionProblem(value: string): 'offline' | 'reconnecting' | undefined {
  if (value.startsWith('Offline')) return 'offline';
  if (value.startsWith('Reconnecting') || value.startsWith('Cannot reach server')) return 'reconnecting';
  return undefined;
}
let retryNow: (() => void) | undefined;
/** Try to reconnect at once rather than waiting for the next attempt. */
export function reconnectNow() { retryNow?.(); }
// An idle stream sends no batches, only an `alive` event after 15 s without one
// (src/identity/chat/sync.rs). Reconnect once two of them are missed, not sooner.
const SILENT_STREAM_LIMIT_MS = 45_000;
// A new stream sends its first batch at once, so a connect that stays silent
// this long has hung and is retried sooner.
const CONNECT_LIMIT_MS = 15_000;
const delivered = new Map<string, Set<string>>(), seen = new Map<string, Set<string>>();
let receiptBusy = false;
function saveReceipts() {
  try { localStorage.setItem(storageScope('relay.pending-receipts'), JSON.stringify({ delivered: [...delivered].map(([chat,ids])=>[chat,[...ids]]), seen: [...seen].map(([chat,ids])=>[chat,[...ids]]) })); } catch { /* Unconfirmed receipts remain in memory; never infer delivery from a failed save. */ }
}
function restoreReceipts() {
  try { const stored = JSON.parse(localStorage.getItem(storageScope('relay.pending-receipts')) ?? '{}');
    for (const [name,map] of [['delivered',delivered],['seen',seen]] as const) for (const [chat, ids] of stored[name] ?? []) if (typeof chat === 'string' && Array.isArray(ids)) map.set(chat,new Set(ids.filter((id: unknown)=>typeof id==='string')));
  } catch { /* A corrupt local receipt queue cannot mark messages seen. */ }
}
export function acknowledge(chat: string, messages: Message[], read = false) {
  const map = read ? seen : delivered, ids = map.get(chat) ?? new Set<string>();
  for (const message of messages) if (message.author !== workspaceSession?.user?.id && ['sent', 'complete'].includes(message.state)) ids.add(message.id);
  if (ids.size) { map.set(chat, ids); saveReceipts(); }
}
async function flushReceipts() {
  if (receiptBusy) return;
  receiptBusy = true;
  try {
    for (const [map, read] of [[seen, true], [delivered, false]] as const) {
      for (const [chat, pending] of map) {
        const ids = [...pending].slice(0, 100);
        if (!ids.length) continue;
        try {
          await chatRequest(`/chats/${chat}/receipts`, { ids, seen: read }, AbortSignal.timeout(10_000));
          ids.forEach(id => pending.delete(id));
          if (!pending.size) map.delete(chat);
          if (read) ids.forEach(id => delivered.get(chat)?.delete(id));
          saveReceipts();
        } catch (e) {
          if (e instanceof Error && 'status' in e && [400, 401, 403, 404].includes(Number(e.status))) map.delete(chat);
          saveReceipts(); return;
        }
      }
    }
  } finally { receiptBusy = false; }
}
export function startLive() {
  if (!workspaceId || !workspaceSession?.user) return;
  restoreReceipts();
  let fallback: number | undefined, receiptsEnabled = false;
  let stopped = false, stream: EventSource | undefined, retry: number | undefined, cursor: string | undefined;
  try { cursor = sessionStorage.getItem(storageScope('relay.sync-cursor')) ?? undefined; } catch { /* A fresh snapshot is safe. */ }
  let failures = 0, lastEvent = Date.now();
  // Messages replayed since the connection came back after a visible drop;
  // undefined while connected without a gap.
  let caughtUp: Set<string> | undefined;
  const connect = () => {
    if (stopped || !window.navigator.onLine) return;
    lastEvent = Date.now(); setStatus('Connecting…');
    stream = new EventSource(`/api/identity/workspaces/${workspaceId}/message-events?compact=true${cursor ? `&after=${cursor}` : ''}`);
    stream.addEventListener('changes', event => {
      if (stopped) return;
      lastEvent = Date.now();
      const data = JSON.parse((event as MessageEvent).data) as { cursor: number; more?: boolean; reset?: boolean; events: Change[] };
      // A cursor older than the server keeps changes for: screens reload instead.
      if (data.reset && cursor) { caughtUp = undefined; showNotice({ kind: 'reloaded' }); }
      // Publish before advancing: reconnection can safely replay anything not applied.
      if (data.reset) subscribers.forEach(fn => fn({ kind: 'reset' }));
      for (const change of data.events) {
        subscribers.forEach(fn => fn(change));
        if (change.chat && change.message) acknowledge(change.chat, [change.message]);
        if (change.kind === 'message' && change.message) caughtUp?.add(change.message.id);
      }
      if (caughtUp && !data.more) { showNotice({ kind: 'back', messages: caughtUp.size }); caughtUp = undefined; }
      cursor = String(data.cursor);
      try { sessionStorage.setItem(storageScope('relay.sync-cursor'), cursor); } catch { /* Reconnect within this page still replays. */ }
      failures = 0; setStatus('Connected');
    });
    // A sign of life only: no change to publish and no status to show.
    stream.addEventListener('alive', () => { lastEvent = Date.now(); });
    // Sent first on every stream: the web build the server now hands out.
    stream.addEventListener('version', event => {
      lastEvent = Date.now();
      try { if (outdated((JSON.parse((event as MessageEvent).data) as { client?: unknown }).client)) showNotice({ kind: 'updated' }); } catch { /* The next stream says it again. */ }
    });
    // Who is online and their statuses: everyone after connecting, then each change.
    stream.addEventListener('presence', event => {
      if (stopped) return;
      lastEvent = Date.now();
      try { applyPresence((JSON.parse((event as MessageEvent).data) as { people?: unknown }).people); } catch { /* The next snapshot repairs it. */ }
    });
    const reconnect = () => {
      if (stopped) return;
      stream?.close(); caughtUp ??= new Set(); setStatus('Reconnecting… Messages remain on this device until confirmed.');
      retry = window.setTimeout(connect, Math.min(30_000, 1000 * 2 ** Math.min(failures++, 5)) + Math.random() * 500);
    };
    stream.onerror = reconnect;
    stream.addEventListener('unavailable', () => { if (stopped) return; subscribers.forEach(fn => fn({ kind: 'access' })); reconnect(); });
  };
  const unsubscribeSent = subscribeSent((chat, message) => subscribers.forEach(fn => fn(message ? { kind: 'message', chat, message } : { kind: 'reset' })));
  const detect = async () => {
    try {
      const capability = await chatRequest<{ live: boolean }>('/message-capabilities');
      if (stopped) return;
      receiptsEnabled = capability.live;
      if (capability.live) connect();
      else {
        setStatus('Standard refresh · live delivery requires PostgreSQL');
        fallback = window.setInterval(() => { if (!document.hidden) subscribers.forEach(fn => fn({ kind: 'reset' })); }, 5000);
      }
    } catch {
      if (!stopped) { setStatus('Cannot reach server. Pending messages are kept on this device.'); retry = window.setTimeout(() => void detect(), 3000); }
    }
  };
  void detect();
  const timer = window.setInterval(() => {
    if (receiptsEnabled && ['Connected', 'Connecting…'].includes(status) && window.navigator.onLine && Date.now() - lastEvent > (status === 'Connecting…' ? CONNECT_LIMIT_MS : SILENT_STREAM_LIMIT_MS)) { stream?.close(); clearTimeout(retry); caughtUp ??= new Set(); setStatus('Reconnecting…'); connect(); }
    if (receiptsEnabled) void flushReceipts(); void flushOutbox().catch(() => setStatus('Local message storage unavailable. Keep your draft.')); }, 1000);
  const online = () => { stream?.close(); clearTimeout(retry); if (receiptsEnabled) connect(); else void detect(); void flushOutbox().catch(() => {}); };
  const offline = () => { stream?.close(); clearTimeout(retry); caughtUp ??= new Set(); setStatus('Offline · messages are kept on this device until confirmed.'); };
  window.addEventListener('online', online); window.addEventListener('offline', offline);
  retryNow = () => { failures = 0; online(); };
  return () => { stopped = true; retryNow = undefined; unsubscribeSent(); stream?.close(); clearTimeout(retry); window.clearInterval(timer); window.clearInterval(fallback); window.removeEventListener('online', online); window.removeEventListener('offline', offline); delivered.clear(); seen.clear(); stopOutbox(); };
}
