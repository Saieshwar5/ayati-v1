import { useEffect, useRef, useState, type FocusEvent, type PointerEvent } from 'react';
import { subscribeChanges } from './live';
import type { Conversation, Message } from './model';

/** When a live message counts as activity, in seconds (chat_activity.rs): a sent or
 * finished message that is not an agent's progress note. Reactions, receipts and
 * edits arrive as changes to older messages with their old times, so they never
 * move a conversation; a reply that finished late carries `activity_at`. */
export function messageActivity(message: Message): number | undefined {
  if (message.update_of || !['sent', 'complete'].includes(message.state)) return undefined;
  return Math.max(message.at, message.activity_at ?? 0);
}

/** Newest activity first: the list's own times, raised by live messages since it
 * was read. The sort is stable, so ties keep the server's order. */
export function byActivity(chats: Conversation[], live: Record<string, number>): Conversation[] {
  const activity = (chat: Conversation) => Math.max(chat.activity ?? 0, live[chat.id] ?? 0);
  return [...chats].sort((a, b) => activity(b) - activity(a));
}

/** How long rows hold still after the mouse last moved over the list, so a cursor
 * left resting on it does not freeze the order while you read. */
export const HOLD_MS = 1500;

const keyboardFocus = (target: EventTarget) => {
  try { return (target as Element).matches(':focus-visible'); } catch { return true; }
};

/** The Chat list's order. A live message moves its conversation up straight away,
 * and one for a conversation the list doesn't have yet (a new direct message, a
 * channel you were added to) reads the list again, once per conversation. While a
 * mouse moves over the list or the keyboard moves through it, rows hold still so
 * nothing moves under the pointer or focus; they catch up on leaving, or once the
 * mouse has rested for `HOLD_MS`. */
export function useConversationOrder(chats: Conversation[], reload: { current: () => Promise<void> }) {
  const [live, setLive] = useState<Record<string, number>>({});
  const [held, setHeld] = useState<string[]>();
  const pointer = useRef(false), keyboard = useRef(false), rest = useRef<number>(undefined);
  useEffect(() => () => window.clearTimeout(rest.current), []);
  const known = useRef(new Set<string>()), asked = useRef(new Set<string>());
  useEffect(() => { known.current = new Set(chats.map(chat => chat.id)); }, [chats]);
  useEffect(() => subscribeChanges(change => {
    const chat = change.chat, message = change.message;
    if (change.kind !== 'message' || !chat || !message) return;
    const at = messageActivity(message);
    if (at !== undefined) setLive(previous => (previous[chat] ?? 0) >= at ? previous : { ...previous, [chat]: at });
    if (!known.current.has(chat) && !asked.current.has(chat)) { asked.current.add(chat); void reload.current(); }
  }), [reload]);
  const sorted = byActivity(chats, live);
  const rank = held && new Map(held.map((id, index) => [id, index]));
  // Conversations that appeared while held go first, as they would once released.
  const shown = rank ? [...sorted].sort((a, b) => (rank.get(a.id) ?? -1) - (rank.get(b.id) ?? -1)) : sorted;
  const sync = () => setHeld(previous => pointer.current || keyboard.current ? previous ?? shown.map(chat => chat.id) : undefined);
  const moving = (event: PointerEvent) => {
    if (event.pointerType !== 'mouse') return;
    pointer.current = true; sync();
    window.clearTimeout(rest.current);
    rest.current = window.setTimeout(() => { pointer.current = false; sync(); }, HOLD_MS);
  };
  const listeners = {
    onPointerEnter: moving,
    onPointerMove: moving,
    onPointerLeave: (event: PointerEvent) => { if (event.pointerType === 'mouse') { window.clearTimeout(rest.current); pointer.current = false; sync(); } },
    onFocus: (event: FocusEvent) => { if (keyboardFocus(event.target)) { keyboard.current = true; sync(); } },
    onBlur: (event: FocusEvent) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) { keyboard.current = false; sync(); } },
  };
  return { chats: shown, listeners };
}
