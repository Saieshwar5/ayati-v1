import { storageScope, workspaceSession } from '../../workspace-context';
import { chatRequest, type Message } from './model';
export type Submission = { id: string; content: string; agent?: string; thread?: string; attachments: string[] };
export type PendingMessage = { key: string; scope: string; chat: string; payload: Submission; at: number; state: 'waiting' | 'sending' | 'checking' | 'failed' | 'paused' | 'sent'; error?: string; attempts: number; next: number };
const sentListeners = new Set<(chat: string, message?: Message) => void>();
export function subscribeSent(fn: (chat: string, message?: Message) => void) { sentListeners.add(fn); return () => { sentListeners.delete(fn); }; }
const listeners = new Set<() => void>();
let entries: PendingMessage[] = [];
let loading: Promise<void> | undefined;
let generation = 0;
let running = false;
export const outboxSnapshot = () => entries;
export const subscribeOutbox = (fn: () => void) => { listeners.add(fn); return () => { listeners.delete(fn); }; };
const notify = () => listeners.forEach(fn => fn());
const scope = () => storageScope('relay.outbox');
function database(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open('ayati-message-outbox', 1);
    request.onupgradeneeded = () => request.result.createObjectStore('messages', { keyPath: 'key' });
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(new Error('Cannot save this message on this device. Keep your draft and try again.'));
  });
}
async function store(item: PendingMessage, remove = false) {
  const db = await database();
  try { await new Promise<void>((resolve, reject) => {
    const tx = db.transaction('messages', 'readwrite');
    if (remove) tx.objectStore('messages').delete(item.key); else tx.objectStore('messages').put(item);
    tx.oncomplete = () => resolve(); tx.onerror = () => reject(new Error('Local message storage failed. Your draft has not been cleared.')); tx.onabort = tx.onerror;
  }); } finally { db.close(); }
}
export async function loadOutbox() {
  if (loading) return loading;
  const identity = scope(), version = generation;
  loading = (async () => {
    const db = await database();
    try {
      const all = await new Promise<PendingMessage[]>((resolve, reject) => {
        const request = db.transaction('messages').objectStore('messages').getAll();
        request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error);
      });
      if (version !== generation || identity !== scope()) return;
      entries = all.filter(item => item.scope === identity).map(item => ({ ...item, state: item.state === 'sending' ? 'checking' : item.state })); notify();
    } finally { db.close(); }
  })();
  try { await loading; } catch (e) { loading = undefined; throw e; }
}
export async function enqueue(chat: string, payload: Submission) {
  if (!workspaceSession?.user) throw new Error('Sign in before sending.');
  await loadOutbox();
  if (entries.length >= 500) throw new Error('There are 500 unresolved messages on this device. Resolve pending sends before adding more.');
  const item: PendingMessage = { key: `${scope()}.${payload.id}`, scope: scope(), chat, payload, at: Date.now(), state: 'waiting', attempts: 0, next: 0 };
  await store(item);
  entries = [...entries.filter(old => old.key !== item.key), item]; notify();
  void flushOutbox().catch(() => {});
}
async function update(item: PendingMessage, remove = false) {
  await store(item, remove);
  if (item.scope !== scope()) return;
  entries = remove ? entries.filter(old => old.key !== item.key) : entries.map(old => old.key === item.key ? item : old); notify();
}
export async function retryMessage(item: PendingMessage) {
  await update({ ...item, state: 'waiting', next: 0, error: undefined, at: Date.now() });
  void flushOutbox().catch(() => {});
}
export async function flushOutbox() {
  if (running || !workspaceSession?.user) return;
  running = true;
  const version = generation;
  try {
    await loadOutbox();
    for (const original of [...entries]) {
      if (version !== generation || original.scope !== scope()) break;
      if (['failed', 'paused', 'sent'].includes(original.state) || original.next > Date.now()) continue;
      if (original.payload.agent && Date.now() - original.at > 15 * 60_000) {
        await update({ ...original, state: 'paused', error: 'This agent request is over 15 minutes old. Review it before retrying.' }); continue;
      }
      const item = { ...original, state: 'sending' as const };
      await update(item);
      if (version !== generation || item.scope !== scope()) break;
      try {
        const result = await chatRequest<{ message?: Message }>(`/chats/${item.chat}/messages`, item.payload, AbortSignal.timeout(15_000));
        if (version !== generation || item.scope !== scope()) break;
        if (result.message) {
          sentListeners.forEach(fn => fn(item.chat, result.message));
          await update(item, true);
        } else {
          await update({ ...item, state: 'sent', error: undefined });
          sentListeners.forEach(fn => fn(item.chat));
        }
      } catch (error) {
        const status = error instanceof Error && 'status' in error ? Number(error.status) : 0;
        const retryHeader = error instanceof Error && 'retryAfter' in error ? String(error.retryAfter ?? '') : '';
        const retryDelay = /^\d+$/.test(retryHeader) ? Number(retryHeader)*1000 : Math.max(0, Date.parse(retryHeader)-Date.now()) || 0;
        const temporary = !status || status >= 500 || status === 429 || status === 408;
        await update({ ...item, attempts: item.attempts + 1, state: temporary ? 'checking' : status === 401 || item.attempts > 0 ? 'paused' : 'failed',
          next: Date.now() + Math.max(retryDelay, Math.min(30_000, 1000 * 2 ** Math.min(item.attempts, 5))) + Math.random() * 1000,
          error: temporary ? 'Server acceptance is unconfirmed. Reconnecting and checking the same message.' : `${item.attempts > 0 ? 'Earlier delivery is unconfirmed. Sending stopped: ' : ''}${error instanceof Error ? error.message : 'Message was rejected.'}` });
        if (temporary || status === 401) break;
      }
    }
  } finally { running = false; }
}
export function stopOutbox() { generation++; entries = []; loading = undefined; notify(); }

export async function confirmSent(messages: Message[]) {
  const ids = new Set(messages.filter(m => m.author === workspaceSession?.user?.id).map(m => m.id));
  for (const item of [...entries]) if (item.state === 'sent' && ids.has(item.payload.id)) await update(item, true);
}
