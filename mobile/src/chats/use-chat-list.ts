/**
 * The conversations this person can open, who is in them, and unread counts.
 * The three reads run together. Unread counts are read again 500 ms after a
 * live change that can affect them; access changes reload everything.
 */
import type { Change, Conversation, Directory } from '@cuckoo/shared';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useChanges } from '../live/live-context';
import { useWorkspace } from '../session/session-context';
import { useResource } from '../ui/requests';

export type ChatList = { directory: Directory; chats: Conversation[] };

export function useChatList() {
  const { api, user } = useWorkspace();
  const list = useResource(
    useCallback(
      async (signal: AbortSignal): Promise<ChatList> => {
        const [directory, chats] = await Promise.all([api.directory(signal), api.chats(signal)]);
        return { directory, chats: chats.chats };
      },
      [api],
    ),
  );
  const [unread, setUnread] = useState<Record<string, number>>({});
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const abort = useRef<AbortController | undefined>(undefined);

  const readUnread = useCallback(async () => {
    abort.current?.abort();
    const current = new AbortController();
    abort.current = current;
    try {
      const data = await api.unread(current.signal);
      if (!current.signal.aborted) setUnread(Object.fromEntries(data.unread.map(item => [item.chat, item.count])));
    } catch {
      // An outage keeps the last counts.
    }
  }, [api]);

  useEffect(() => {
    void readUnread();
    return () => {
      clearTimeout(timer.current);
      abort.current?.abort();
    };
  }, [readUnread]);

  const refreshList = list.refresh;
  useChanges(
    useCallback(
      (change: Change) => {
        if (change.kind === 'access' || change.kind === 'reset') void refreshList();
        const affectsUnread =
          change.kind === 'access' || change.kind === 'reset' || (change.kind === 'message' && change.message?.author !== user.id);
        if (affectsUnread) {
          clearTimeout(timer.current);
          timer.current = setTimeout(() => void readUnread(), 500);
        }
      },
      [refreshList, readUnread, user.id],
    ),
  );

  const refresh = useCallback(async () => {
    await Promise.all([list.refresh(), readUnread()]);
  }, [list, readUnread]);

  return { ...list, unread, refresh };
}
