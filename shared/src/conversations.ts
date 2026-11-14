/**
 * How a conversation is named and grouped for one viewer. The same rules as
 * the web (web/src/react/chat/model.ts), with the viewer passed in.
 */
import type { Conversation, Directory, Message, Participant } from './types';

export type ConversationKind = 'Channel' | 'Person' | 'Agent';
export type ChatFilter = 'all' | 'unread' | 'agents' | 'people' | 'channels';

function otherPerson(chat: Conversation, directory: Directory, viewer: string): Participant | undefined {
  return directory.users.find(user => chat.users.includes(user.id) && user.id !== viewer);
}

export function conversationName(chat: Conversation, directory: Directory, viewer: string): string {
  if (chat.kind !== 'direct') return chat.name;
  const agent = directory.agents.find(agent => chat.agents.includes(agent.id));
  return otherPerson(chat, directory, viewer)?.name ?? agent?.name ?? chat.name;
}

export function conversationKind(chat: Conversation, directory: Directory, viewer: string): ConversationKind {
  if (chat.kind !== 'direct') return 'Channel';
  return !otherPerson(chat, directory, viewer) && chat.agents.length ? 'Agent' : 'Person';
}

/** The agent a direct conversation is with, which every message there asks. */
export function directAgent(chat: Conversation, directory: Directory, viewer: string): string | undefined {
  if (conversationKind(chat, directory, viewer) !== 'Agent') return undefined;
  return directory.agents.find(agent => chat.agents.includes(agent.id))?.id;
}

export function filterConversations(
  chats: Conversation[],
  filter: ChatFilter,
  directory: Directory,
  viewer: string,
  unread: Record<string, number>,
): Conversation[] {
  return chats.filter(chat => {
    if (chat.archived) return false;
    switch (filter) {
      case 'unread':
        return (unread[chat.id] ?? 0) > 0;
      case 'agents':
        return conversationKind(chat, directory, viewer) === 'Agent';
      case 'people':
        return conversationKind(chat, directory, viewer) === 'Person';
      case 'channels':
        return conversationKind(chat, directory, viewer) === 'Channel';
      default:
        return true;
    }
  });
}

/**
 * Apply one message from the live stream to a loaded history: replace the
 * message with the same id when this copy is as new or newer, otherwise add it
 * in time order. Returns the same array when nothing changes.
 */
export function mergeMessage(messages: Message[], next: Message): Message[] {
  const index = messages.findIndex(message => message.id === next.id);
  if (index >= 0) {
    const current = messages[index];
    if ((current.revision ?? 0) > (next.revision ?? 0)) return messages;
    const copy = messages.slice();
    copy[index] = { ...current, ...next };
    return copy;
  }
  const copy = [...messages, next];
  copy.sort((a, b) => a.at - b.at || (a.sequence ?? 0) - (b.sequence ?? 0));
  return copy;
}

/** The viewer's existing direct conversation with this agent, if any (as on the web). */
export function existingAgentChat(chats: Conversation[], agent: string, viewer: string): Conversation | undefined {
  return chats.find(
    chat =>
      chat.kind === 'direct' &&
      chat.agents.length === 1 &&
      chat.agents[0] === agent &&
      chat.users.length === 1 &&
      chat.users[0] === viewer,
  );
}

/** Message text with mention links shown as `@Name` (the web's mention markup). */
export function plainText(content: string): string {
  return content.replace(/\[@([^\]\n]+)\]\(mention:(?:user|agent):[a-zA-Z0-9_-]+\)/g, '@$1');
}

/** Stored as the content of a message that only shares files. */
export const filesOnlyContent = 'Shared files';
