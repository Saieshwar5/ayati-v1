/**
 * One conversation: who it is with, its history (newest page first, older
 * pages on demand) and live changes merged in by message id. Messages from
 * others are marked seen while the conversation is open.
 */
import { mergeMessage, type Change, type Conversation, type Directory, type Message } from '@cuckoo/shared';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useChanges } from '../live/live-context';
import { useWorkspace } from '../session/session-context';
import { errorMessage } from '../ui/requests';

type Loaded = { chat: Conversation; directory: Directory; messages: Message[]; before: number | null };

export function useConversation(id: string) {
  const { api, user } = useWorkspace();
  const [loaded, setLoaded] = useState<Loaded>();
  const [error, setError] = useState('');
  const [olderBusy, setOlderBusy] = useState(false);
  const abort = useRef<AbortController | undefined>(undefined);
  const seen = useRef(new Set<string>());

  const load = useCallback(async () => {
    abort.current?.abort();
    const current = new AbortController();
    abort.current = current;
    try {
      const [directory, chats, history] = await Promise.all([
        api.directory(current.signal),
        api.chats(current.signal),
        api.messages(id, undefined, current.signal),
      ]);
      if (current.signal.aborted) return;
      const chat = chats.chats.find(c => c.id === id);
      if (!chat) throw new Error('This conversation is no longer available to you.');
      setLoaded({ chat, directory, messages: history.messages, before: history.next_before ?? null });
      setError('');
    } catch (failure) {
      if (!current.signal.aborted) setError(errorMessage(failure));
    }
  }, [api, id]);

  useEffect(() => {
    void load();
    return () => abort.current?.abort();
  }, [load]);

  useChanges(
    useCallback(
      (change: Change) => {
        if (change.kind === 'reset' || change.kind === 'access') return void load();
        if (change.kind !== 'message' || change.chat !== id || !change.message) return;
        const message = change.message;
        setLoaded(current => (current ? { ...current, messages: mergeMessage(current.messages, message) } : current));
      },
      [id, load],
    ),
  );

  // Seen receipts for others' finished messages, at most once each.
  const messages = loaded?.messages;
  useEffect(() => {
    if (!messages) return;
    const ids = messages
      .filter(m => m.author !== user.id && ['sent', 'complete'].includes(m.state) && !seen.current.has(m.id))
      .map(m => m.id)
      .slice(-100);
    if (!ids.length) return;
    ids.forEach(m => seen.current.add(m));
    api.receipts(id, ids, true).catch(() => ids.forEach(m => seen.current.delete(m)));
  }, [messages, api, id, user.id]);

  const loadOlder = useCallback(async () => {
    if (!loaded?.before || olderBusy) return;
    setOlderBusy(true);
    try {
      const page = await api.messages(id, loaded.before);
      setLoaded(current =>
        current
          ? {
              ...current,
              messages: page.messages.reduce(mergeMessage, current.messages),
              before: page.next_before ?? null,
            }
          : current,
      );
    } catch {
      // Scrolling up again retries.
    } finally {
      setOlderBusy(false);
    }
  }, [api, id, loaded?.before, olderBusy]);

  /** A message the server accepted, shown before its live copy arrives. */
  const add = useCallback((message: Message) => {
    setLoaded(current => (current ? { ...current, messages: mergeMessage(current.messages, message) } : current));
  }, []);

  return { loaded, error, reload: load, loadOlder, add };
}
