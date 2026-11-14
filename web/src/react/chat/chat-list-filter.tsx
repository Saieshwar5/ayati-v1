// Which conversations the Chat list shows: everything, or only what is unread,
// and never a conversation you hid until something mentions you there
// ("Nothing, and hide it", schema 116). The open conversation always shows.
import { useState } from 'react';
import { storageScope } from '../../workspace-context';
import type { Conversation } from './model';
import type { Level } from './notifications';

const key = () => storageScope('relay.chat.unread-only');
function remembered() { try { return localStorage.getItem(key()) === 'true'; } catch { return false; } }

export function useChatListFilter({ chats, selected, unread, mentioned, levels }: {
  chats: Conversation[]; selected: string | null; unread: Record<string, number>;
  mentioned: Record<string, number>; levels: Record<string, Level>;
}) {
  const [unreadOnly, setUnreadOnlyState] = useState(remembered);
  const [showHidden, setShowHidden] = useState(false);
  // A per-person convenience: an unavailable store just forgets it.
  const setUnreadOnly = (on: boolean) => { setUnreadOnlyState(on); try { localStorage.setItem(key(), String(on)); } catch { /* Forgotten on reload. */ } };
  const active = chats.filter(chat => !chat.archived);
  const hidden = (chat: Conversation) => levels[chat.id] === 'hide' && !mentioned[chat.id];
  const listed = active.filter(chat => !hidden(chat) || showHidden);
  const unreadChats = listed.filter(chat => (unread[chat.id] ?? 0) > 0).length;
  const visible = (chat: Conversation) => chat.id === selected || chat.archived ? true
    : hidden(chat) && !showHidden ? false
    : !unreadOnly || (unread[chat.id] ?? 0) > 0;
  return {
    visible, unreadOnly, setUnreadOnly, unreadChats,
    hiddenCount: active.filter(hidden).length, showHidden, setShowHidden,
    readCount: listed.length - unreadChats,
  };
}
export type ChatListFilter = ReturnType<typeof useChatListFilter>;

export function UnreadOnlySwitch({ filter }: { filter: ChatListFilter }) {
  return <label className="chat-unread-only">
    <span>Unread only</span>
    {filter.unreadChats > 0 && <small className="chat-unread-total" aria-label={`${filter.unreadChats} unread conversations`}>{filter.unreadChats}</small>}
    <input type="checkbox" role="switch" checked={filter.unreadOnly} onChange={event => filter.setUnreadOnly(event.target.checked)} />
  </label>;
}

/** Says why the list is shorter than usual, so it never just looks empty. */
export function ChatListNote({ filter }: { filter: ChatListFilter }) {
  const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;
  return <>
    {filter.unreadOnly && <p className="chat-help chat-list-note" role="status">
      {filter.unreadChats === 0 ? 'You’re all caught up. ' : ''}
      {filter.readCount > 0 && `${plural(filter.readCount, 'other conversation is', 'other conversations are')} read. `}
      <button type="button" className="chat-thread-link" onClick={() => filter.setUnreadOnly(false)}>Show all</button>
    </p>}
    {filter.hiddenCount > 0 && <p className="chat-help chat-list-note">
      {plural(filter.hiddenCount, 'conversation is', 'conversations are')} hidden until someone mentions you.{' '}
      <button type="button" className="chat-thread-link" aria-expanded={filter.showHidden} onClick={() => filter.setShowHidden(!filter.showHidden)}>{filter.showHidden ? 'Hide them' : 'Show them'}</button>
    </p>}
  </>;
}
