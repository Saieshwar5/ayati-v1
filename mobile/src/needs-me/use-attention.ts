/**
 * What waits for this person (approvals, questions, problems) or finished
 * recently, from `…/attention`. Read again a second after live changes, since
 * an agent's message is how a new item appears.
 */
import type { AttentionView, Change } from '@cuckoo/shared';
import { useCallback, useEffect, useRef } from 'react';
import { useChanges } from '../live/live-context';
import { useWorkspace } from '../session/session-context';
import { useResource } from '../ui/requests';

export function useAttention(view: AttentionView) {
  const { api } = useWorkspace();
  const resource = useResource(useCallback((signal: AbortSignal) => api.attention(view, null, signal), [api, view]));
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const refresh = resource.refresh;
  useEffect(() => () => clearTimeout(timer.current), []);
  useChanges(
    useCallback(
      (change: Change) => {
        if (!['message', 'access', 'reset'].includes(change.kind)) return;
        clearTimeout(timer.current);
        timer.current = setTimeout(() => void refresh(), 1000);
      },
      [refresh],
    ),
  );
  return resource;
}

export type Kind = { label: string; tone: 'danger' | 'working' | 'warning' | 'success' };

/** Colour is always paired with words (accessibility rule). */
export function attentionKind(item: { kind: string; status: string }): Kind {
  if (item.kind === 'approval') return { label: item.status === 'uncertain' ? 'Outcome unclear' : 'Approval', tone: 'danger' };
  if (item.kind === 'question') return { label: 'Question', tone: 'working' };
  if (item.status === 'complete') return { label: 'Finished', tone: 'success' };
  return { label: 'Problem', tone: 'warning' };
}
