import { chatAttention, useUnreadCounts } from './unread-counts';

/** The rail's Chat label with its unread count, so a message is noticed from any
 * page. The words are the link's accessible name; the digits are for sight only.
 * The count turns red when something is for the viewer, as in the conversation
 * list. It is its own component so a new count redraws only this label. */
export function ChatNavLabel() {
  const { count, forMe } = chatAttention(useUnreadCounts());
  if (count === 0) return <span>Chat</span>;
  const words = `Chat, ${count} unread ${count === 1 ? 'message' : 'messages'}${forMe > 0 ? `, ${forMe} for you` : ''}`;
  return <><span>{words}</span>
    <small className={`workspace-nav-unread${forMe > 0 ? ' workspace-nav-unread-mention' : ''}`} aria-hidden="true">{count > 99 ? '99+' : count}</small></>;
}
