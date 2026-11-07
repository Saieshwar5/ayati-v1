// What the rail's Chat item and the tab title show (D245), derived from the
// shared unread store during rendering.
import { useEffect } from 'react';
import { useConversationSettings, useUnreadCounts, type ConversationSettings, type UnreadCounts } from './unread-counts';
import { defaultLevel } from './notifications';

/** A count of what is for you, or only a dot when other messages are unread. */
export type ChatBadge = { count: number; dot: boolean };

/** Unread messages in direct conversations plus @mentions and replies to you in
 * channels and groups. A direct message that mentions you counts once. Other
 * unread channel and group messages light the dot when nothing is counted.
 * Muted conversations (level `none`, saved or by default) add nothing. A
 * conversation the list does not have yet counts only what is for you. */
export function chatBadge({ counts, mentions }: UnreadCounts, { chats, levels, defaults }: ConversationSettings): ChatBadge {
  let count = 0, other = false;
  for (const [id, unread] of Object.entries(counts)) {
    const chat = chats.get(id);
    if (unread <= 0 || (levels[id] ?? (chat && defaultLevel(chat, defaults))) === 'none') continue;
    const forYou = chat?.kind === 'direct' ? unread : Math.min(mentions[id] ?? 0, unread);
    count += forYou;
    if (forYou < unread) other = true;
  }
  return { count, dot: count === 0 && other };
}

export const badgeText = (count: number) => count > 99 ? '99+' : String(count);

/** The accessible name of a navigation item carrying the badge: "Chat, 3 unread". */
export function badgeLabel(label: string, badge: ChatBadge) {
  return badge.count ? `${label}, ${badgeText(badge.count)} unread` : badge.dot ? `${label}, new messages` : label;
}

export const useChatBadge = () => chatBadge(useUnreadCounts(), useConversationSettings());

/** Puts the same count before the title, "(3) Home · Cuckoo", so a background
 * tab still tells you; the previous title returns on unmount. */
export function useUnreadTitle(title: string) {
  const { count } = useChatBadge();
  const shown = count ? `(${badgeText(count)}) ${title}` : title;
  useEffect(() => {
    const previous = document.title;
    document.title = shown;
    return () => { document.title = previous; };
  }, [shown]);
}
