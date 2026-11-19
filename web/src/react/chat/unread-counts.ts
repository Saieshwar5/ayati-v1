import { useSyncExternalStore } from 'react';
import { workspaceSession } from '../../workspace-context';
import { subscribeChanges, type Change } from './live';
import { chatRequest } from './model';

/** `muted` marks conversations whose bell the viewer set to Nothing, by their own
 * choice or their channel default. A server from before the flag mutes none. */
export type UnreadCounts = { counts: Record<string, number>; mentions: Record<string, number>; muted: Record<string, boolean> };
type UnreadResponse = { unread: { chat: string; count: number; mentions?: number; muted?: boolean }[] };
const none: UnreadCounts = { counts: {}, mentions: {}, muted: {} };

/** Whether a live change can alter the viewer's unread counts. A message change
 * from someone else may be a new, finished or deleted message, or the viewer's
 * own seen receipt (receipts arrive as changes to their message, without saying
 * whose). Access changes cover archive, visibility and membership. Receipts and
 * reactions on the viewer's own messages, agent pauses and panel, sample or
 * event access never change what the viewer has unread. */
export function changesUnread(change: Change, viewer: string | undefined): boolean {
  if (change.kind === 'access' || change.kind === 'reset') return true;
  return change.kind === 'message' && change.message?.author !== viewer;
}

// One store for the workspace: the rail's Chat icon and the conversation list
// show the same read. It reads while anything shows the counts and forgets them
// when nothing does. Changing account or workspace reloads the page.
let unread = none;
const listeners = new Set<() => void>();
let stop: (() => void) | undefined;
let readAgain: (() => void) | undefined;

/** Reads the counts, and again 500 ms after a change that can affect them. One
 * read runs at a time, so an older answer never replaces a newer one; a change
 * during a read starts one more afterwards. Returns its cleanup. */
function start(): () => void {
  const abort = new AbortController(), viewer = workspaceSession?.user?.id;
  let busy = false, again = false, timer: number | undefined;
  const show = (next: UnreadCounts) => { unread = next; listeners.forEach(listener => listener()); };
  const read = async () => {
    if (busy) { again = true; return; }
    busy = true;
    try {
      const data = await chatRequest<UnreadResponse>('/unread', undefined, abort.signal);
      if (!abort.signal.aborted) show({
        counts: Object.fromEntries(data.unread.map(item => [item.chat, item.count])),
        mentions: Object.fromEntries(data.unread.map(item => [item.chat, item.mentions ?? 0])),
        muted: Object.fromEntries(data.unread.map(item => [item.chat, item.muted === true])),
      });
    } catch (e) {
      // Denied or removed access clears the counts; an outage keeps the last ones.
      if (!abort.signal.aborted && e instanceof Error && 'status' in e && [400, 401, 403, 404].includes(Number(e.status))) show(none);
    } finally {
      busy = false;
      if (again && !abort.signal.aborted) { again = false; void read(); }
    }
  };
  void read();
  const unsubscribe = subscribeChanges(change => {
    if (!changesUnread(change, viewer) || timer !== undefined) return;
    timer = window.setTimeout(() => { timer = undefined; void read(); }, 500);
  });
  readAgain = () => void read();
  return () => { abort.abort(); unsubscribe(); window.clearTimeout(timer); readAgain = undefined; unread = none; };
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  if (listeners.size === 1) stop = start();
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) { stop?.(); stop = undefined; }
  };
}

/** Per-conversation unread and mention counts, shared by everything showing them. */
export function useUnreadCounts(): UnreadCounts {
  return useSyncExternalStore(subscribe, () => unread);
}

/** Reads the counts again after the viewer changes a bell level or their channel
 * default on this page: no live change announces it, and it changes `muted`. */
export function refreshUnread(): void { readAgain?.(); }

/** What the rail's Chat icon shows. Muted conversations add only what is for the
 * viewer (an @mention or an agent's reply to their request); every other
 * conversation adds all its unread messages. */
export function chatAttention(unread: UnreadCounts): { count: number; forMe: number } {
  let count = 0, forMe = 0;
  for (const [chat, total] of Object.entries(unread.counts)) {
    const mine = unread.mentions[chat] ?? 0;
    count += unread.muted[chat] ? mine : total;
    forMe += mine;
  }
  return { count, forMe };
}
