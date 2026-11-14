/** Agents this person may use, with the conversations needed to open or find theirs. */
import { useCallback } from 'react';
import { useWorkspace } from '../session/session-context';
import { useResource } from '../ui/requests';

export function useAgents() {
  const { api } = useWorkspace();
  return useResource(
    useCallback(
      async (signal: AbortSignal) => {
        const [directory, chats] = await Promise.all([api.directory(signal), api.chats(signal)]);
        return { directory, chats: chats.chats };
      },
      [api],
    ),
  );
}
