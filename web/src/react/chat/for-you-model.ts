import { chatRequest } from './model';

/** Why an item is in For you: the server's "for you" rule (channel_threads::for_me). */
export type ForYouReason = 'mention' | 'agent_reply' | 'thread';
export type ForYouItem = {
  id: string; chat: string; chat_name: string; chat_kind: 'channel' | 'group';
  thread: string | null; author: string; kind: 'user' | 'agent'; name: string | null;
  at: number; reason: ForYouReason; unread: boolean;
  /** An agent reply whose sources the person may no longer read: no preview. */
  unavailable: boolean;
  /** At most 400 characters; the message is read where it is. */
  content: string;
};
/** `new` comes only with the first page; `earlier` pages go back 90 days. */
export type ForYouPage = { available: boolean; new: ForYouItem[]; earlier: ForYouItem[]; next_before: string | null };
export type ForYouFilter = 'all' | ForYouReason;

export const loadForYou = (before: string | null, signal?: AbortSignal) =>
  chatRequest<ForYouPage>(`/for-you${before ? `?before=${encodeURIComponent(before)}` : ''}`, undefined, signal);

export const reasonLabel: Record<ForYouReason, string> = { mention: '@mention', agent_reply: 'Reply to your request', thread: 'Thread you follow' };

/** Where the message is: "in #orders", "thread in #design", "in Operations". */
export function itemPlace(item: ForYouItem): string {
  const name = item.chat_kind === 'channel' ? `#${item.chat_name}` : item.chat_name;
  return `${item.thread ? 'thread in' : 'in'} ${name}`;
}

/** Today's items show the time; older ones the date. */
export function itemTime(at: number, now = new Date()): string {
  const when = new Date(at * 1000);
  return when.toDateString() === now.toDateString()
    ? when.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
    : when.toLocaleDateString([], { day: 'numeric', month: 'short' });
}

/** Read items from every loaded page, each once, leaving out any that are new again. */
export function earlierItems(first: ForYouPage, older: ForYouPage[]): ForYouItem[] {
  const fresh = new Set(first.new.map(item => item.id)), seen = new Set<string>();
  return [first, ...older].flatMap(page => page.earlier).filter(item => {
    if (fresh.has(item.id) || seen.has(item.id)) return false;
    seen.add(item.id); return true;
  });
}

/** The For you count: unread messages for the person outside direct messages,
 * the same messages the list's New section holds. */
export function forYouCount(chats: { id: string; kind: string }[], mentions: Record<string, number>): number {
  return chats.reduce((count, chat) => count + (chat.kind === 'direct' ? 0 : mentions[chat.id] ?? 0), 0);
}
