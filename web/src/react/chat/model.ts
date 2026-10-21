import { identityRequest, workspaceId, workspaceSession } from '../../workspace-context';
export type Participant = { id: string; name: string; email?: string; role?: string };
export type Directory = { users: Participant[]; agents: Participant[]; role: string };
export type Conversation = { id: string; name: string; kind: 'group' | 'channel' | 'direct'; description: string; users: string[]; agents: string[] };
export type Message = { id: string; author: string; name: string | null; kind: string; content: string; state: string; at: number; thread?: string | null };
export type History = { messages: Message[]; agents: string[] };
export const chatRequest = <T>(path: string, body?: unknown, signal?: AbortSignal) => identityRequest<T>(`/workspaces/${workspaceId}${path}`, body, signal);
export function conversationName(chat: Conversation, directory: Directory): string {
  if (chat.kind !== 'direct') return chat.name;
  const person = directory.users.find(user => chat.users.includes(user.id) && user.id !== workspaceSession?.user?.id);
  const agent = directory.agents.find(agent => chat.agents.includes(agent.id));
  return person?.name ?? agent?.name ?? chat.name;
}
