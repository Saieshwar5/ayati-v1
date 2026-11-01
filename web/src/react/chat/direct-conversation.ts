import { useEffect, useRef } from 'react';
import { storageScope, workspaceSession, workspaceUrl } from '../../workspace-context';
import { useMutation } from '../requests';
import { chatRequest, type Conversation, type Participant } from './model';

export type DirectTarget = Participant & { kind: 'user' | 'agent' };

export function existingDirect(chats: Conversation[], target: DirectTarget, user: string | undefined) {
  return chats.find(chat => chat.kind === 'direct' && (target.kind === 'user'
    ? chat.agents.length === 0 && chat.users.length === 2 && chat.users.includes(target.id) && !!user && chat.users.includes(user)
    : chat.agents.length === 1 && chat.agents[0] === target.id && chat.users.length === 1 && chat.users[0] === user));
}

/** Reuse the same explicit, guarded opening operation in search and New message. */
export function useDirectConversation(chats: Conversation[], opened: (id: string) => void) {
  const mutation = useMutation();
  const alive = useRef(false);
  const attempt = useRef<{ target: string; id: string }>(undefined);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  /** Finds or creates the direct conversation and returns its ID. */
  const ensure = async (target: DirectTarget) => {
    const existing = existingDirect(chats, target, workspaceSession?.user?.id);
    if (existing) return existing.id;
    const key = JSON.stringify([target.kind, target.id, target.name]);
    if (attempt.current?.target !== key) attempt.current = { target: key, id: crypto.randomUUID() };
    const result = await chatRequest<{ id: string }>('/chats', {
      id: attempt.current.id, kind: 'direct', name: target.name, description: '',
      users: target.kind === 'user' ? [target.id] : [], agents: target.kind === 'agent' ? [target.id] : [],
    });
    return result.id;
  };
  const choose = (target: DirectTarget) => mutation.run(async () => {
    const id = await ensure(target);
    if (alive.current) opened(id);
  });
  return { ...mutation, choose, ensure };
}

/** Open the person's direct chat with an agent (creating it when needed) and start
 * the message with `opening`, never replacing a draft they already have. Used by
 * Activity's "Discuss in chat" and a Home panel's "Ask about this". `attempt` keeps
 * one conversation id per opening, so a double click creates one chat. */
export async function openAgentChat(agent: { id: string; name: string }, opening: string, attempt: { current?: string }) {
  const { chats } = await chatRequest<{ chats: Conversation[] }>('/chats');
  let id = existingDirect(chats, { id: agent.id, name: agent.name, kind: 'agent' }, workspaceSession?.user?.id)?.id;
  if (!id) {
    attempt.current ??= crypto.randomUUID();
    id = (await chatRequest<{ id: string }>('/chats', { id: attempt.current, kind: 'direct', name: agent.name, description: '', users: [], agents: [agent.id] })).id;
  }
  const draft = storageScope(`relay.chat.draft.${id}`);
  try { if (!sessionStorage.getItem(draft)) sessionStorage.setItem(draft, opening); } catch { /* Optional draft. */ }
  location.assign(workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(id)}`));
}
