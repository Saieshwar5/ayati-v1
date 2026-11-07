import { Button } from '../ui';
import type { History, Message } from './model';
import './new-messages.css';

/** Where this visit's unread messages begin in the conversation's main timeline. */
export type UnreadStart = { id: string; count: number; more: boolean };

/** Read once from the first history page: reading the messages marks them seen,
 * and the line must stay where it was while the conversation is open. `more`
 * means unread messages may continue in older pages. */
export function firstUnread(history: History): UnreadStart | null {
  const roots = history.messages.filter(message => !message.thread);
  const unread = roots.filter(message => message.unread);
  if (!unread.length) return null;
  return { id: unread[0].id, count: unread.length, more: roots[0].id === unread[0].id && Boolean(history.next_before) };
}

/** The newest message in a history page, by the server's insertion order. */
export const latestSequence = (messages: Message[]) => messages.reduce((latest, message) => Math.max(latest, message.sequence ?? 0), 0);

/** Whether a live message is new since the conversation opened and should count
 * towards "N new messages": someone else's finished message in the main
 * timeline. An agent's progress notes, thread replies and changes to older
 * messages (reactions, receipts, edits) never count. */
export function countsAsNew(message: Message, viewer: string | undefined, openedAt: number | undefined): boolean {
  return openedAt !== undefined && (message.sequence ?? 0) > openedAt && message.author !== viewer
    && !message.thread && !message.update_of && ['sent', 'complete'].includes(message.state);
}

export function UnreadLine({ start }: { start: UnreadStart }) {
  const label = `${start.count}${start.more ? '+' : ''} unread ${start.count === 1 && !start.more ? 'message' : 'messages'}`;
  return <div className="chat-unread-line" role="separator" aria-label={label}><span aria-hidden="true">{label}</span></div>;
}

/** Floats at the bottom of the scrolled-up message list; it takes no space in it. */
export function JumpToLatest({ count, jump }: { count: number; jump: () => void }) {
  return <div className="chat-jump-latest">
    <Button onClick={jump}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true"><path d="M12 5v14M6 13l6 6 6-6" /></svg>
      {count ? `${count} new ${count === 1 ? 'message' : 'messages'}` : 'Jump to latest'}
    </Button>
  </div>;
}
