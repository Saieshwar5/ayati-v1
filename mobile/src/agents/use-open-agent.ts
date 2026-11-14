/**
 * Open this person's direct conversation with an agent, creating it the first
 * time. One id per agent is reused for retries, so a double tap or an unclear
 * failure can't create two conversations.
 */
import { existingAgentChat, type Conversation, type Participant } from '@cuckoo/shared';
import * as Crypto from 'expo-crypto';
import { useRef } from 'react';
import { useWorkspace } from '../session/session-context';
import { useMutation } from '../ui/requests';

export function useOpenAgent(chats: Conversation[] | undefined, opened: (chat: string) => void) {
  const { api, user } = useWorkspace();
  const mutation = useMutation();
  const attempts = useRef(new Map<string, string>());
  const open = (agent: Participant) =>
    mutation.run(async () => {
      const existing = existingAgentChat(chats ?? [], agent.id, user.id);
      if (existing) return opened(existing.id);
      const id = attempts.current.get(agent.id) ?? Crypto.randomUUID();
      attempts.current.set(agent.id, id);
      const created = await api.createChat({ id, kind: 'direct', name: agent.name, users: [], agents: [agent.id] });
      opened(created.id);
    });
  return { ...mutation, open };
}
