import { identityRequest, workspaceId, workspaceSession } from '../../workspace-context';
export type Participant = { id: string; name: string; email?: string; role?: string; shared?: boolean; scope?: 'private' | 'workspace' };
export type Directory = { users: Participant[]; agents: Participant[]; role: string };
export type Conversation = { id: string; name: string; kind: 'group' | 'channel' | 'direct'; description: string; visibility?: 'private' | 'public'; announcements?: boolean; users: string[]; agents: string[] };
export type Attachment = { id: string; name: string; mime: string; size: number; checksum: string };
export type Message = { id: string; author: string; name: string | null; kind: string; content: string; state: string; at: number; sequence?: number; chat?: string; delivered?: number; seen?: number; attachments?: Attachment[]; thread?: string | null };
export type History = { messages: Message[]; agents: string[]; next_before?: number | null };
export const chatRequest = <T>(path: string, body?: unknown, signal?: AbortSignal) => identityRequest<T>(`/workspaces/${workspaceId}${path}`, body, signal);
export function conversationName(chat: Conversation, directory: Directory): string {
  if (chat.kind !== 'direct') return chat.name;
  const person = directory.users.find(user => chat.users.includes(user.id) && user.id !== workspaceSession?.user?.id);
  const agent = directory.agents.find(agent => chat.agents.includes(agent.id));
  return person?.name ?? agent?.name ?? chat.name;
}
