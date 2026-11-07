import { useEffect, useState } from 'react';
import { workspaceSession } from '../../workspace-context';
import { subscribeChanges, type Change } from './live';
import { chatRequest } from './model';

export type UnreadCounts = { counts: Record<string, number>; mentions: Record<string, number> };
type UnreadResponse = { unread: { chat: string; count: number; mentions?: number }[] };
const none: UnreadCounts = { counts: {}, mentions: {} };

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

/** Per-conversation unread and mention counts, read again 500 ms after a change
 * that can affect them. One read runs at a time, so an older answer never
 * replaces a newer one; a change during a read starts one more afterwards. */
export function useUnreadCounts(): UnreadCounts {
  const [unread, setUnread] = useState<UnreadCounts>(none);
  useEffect(() => {
    const abort = new AbortController(), viewer = workspaceSession?.user?.id;
    let busy = false, again = false, timer: number | undefined;
    const read = async () => {
      if (busy) { again = true; return; }
      busy = true;
      try {
        const data = await chatRequest<UnreadResponse>('/unread', undefined, abort.signal);
        if (!abort.signal.aborted) setUnread({
          counts: Object.fromEntries(data.unread.map(item => [item.chat, item.count])),
          mentions: Object.fromEntries(data.unread.map(item => [item.chat, item.mentions ?? 0])),
        });
      } catch (e) {
        // Denied or removed access clears the counts; an outage keeps the last ones.
        if (!abort.signal.aborted && e instanceof Error && 'status' in e && [400, 401, 403, 404].includes(Number(e.status))) setUnread(none);
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
    return () => { abort.abort(); unsubscribe(); window.clearTimeout(timer); };
  }, []);
  return unread;
}
