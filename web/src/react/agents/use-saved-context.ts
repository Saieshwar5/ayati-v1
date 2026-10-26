import { useEffect, useState } from 'react';
import { identityRequest, storageScope } from '../../workspace-context';
import { errorMessage } from '../requests';
import { base } from './model';
import type { ContextAttempt } from './saved-context';

export function useSavedContext(agent: string, chat: string, attempt: ContextAttempt, open: boolean) {
  const identity = storageScope('saved-task-context');
  const scope = JSON.stringify([identity, agent, chat, attempt.task, attempt.claim]);
  const [state, setState] = useState<{ scope: string; data?: ContextAttempt; error?: string }>({ scope });
  useEffect(() => {
    const controller = new AbortController();
    setState({ scope });
    if (open && attempt.available) {
      const query = new URLSearchParams({ chat, task: attempt.task, claim: attempt.claim });
      void identityRequest<ContextAttempt>(`${base}/${agent}/context-preview?${query}`, undefined, controller.signal)
        .then(data => { if (!controller.signal.aborted) setState({ scope, data }); })
        .catch(error => { if (!controller.signal.aborted) setState({ scope, error: errorMessage(error) }); });
    }
    return () => controller.abort();
  }, [scope, agent, chat, attempt.task, attempt.claim, attempt.available, open]);
  return state.scope === scope && open && attempt.available ? state : { scope };
}
