import { useEffect, useRef, useState } from 'react';
import { identityRequest, storageScope } from '../../workspace-context';
import { errorMessage } from '../requests';
import { subscribeChanges } from '../chat/live';
import { base } from './model';

export type ContextPreview = {
  mode: 'current_preview'; agent: string; conversation: string; assembled_at: number;
  history_policy: 'all' | 'since_join'; message_limit: number;
  knowledge_enabled: boolean; paused: boolean; archived: boolean;
  messages: { id: string; at: number; content: string; thread: string | null;
    author: { id: string; kind: string; display_name: string | null } }[];
  notes: { id: string; revision: number; chat: string | null; title: string; content: string; source: string }[];
};

export function useContextPreview(agent: string, conversation: string) {
  const [data, setData] = useState<ContextPreview>();
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const refresh = useRef(() => {});
  const identity = storageScope('agent-context-preview');
  useEffect(() => {
    let alive = true, generation = 0, loading = false;
    let controller: AbortController | undefined;
    setData(undefined); setError(''); setBusy(false);
    const load = async () => {
      if (!alive || loading || !conversation) return;
      loading = true; setBusy(true);
      const request = ++generation;
      controller = new AbortController();
      const signal = controller.signal;
      try {
        const next = await identityRequest<ContextPreview>(`${base}/${agent}/context-preview?chat=${encodeURIComponent(conversation)}`, undefined, signal);
        if (alive && !signal.aborted && request === generation) { setData(next); setError(''); }
      } catch (failure) {
        if (alive && !signal.aborted && request === generation) { setData(undefined); setError(errorMessage(failure)); }
      } finally {
        if (alive && request === generation) { loading = false; setBusy(false); }
      }
    };
    const invalidate = () => {
      generation++; controller?.abort(); loading = false; setData(undefined);
      void load();
    };
    refresh.current = invalidate;
    void load();
    const unsubscribe = subscribeChanges(change => {
      if (['access', 'reset', 'event_access', 'sample_access', 'panel_access'].includes(change.kind)) invalidate();
    });
    const timer = window.setInterval(() => { if (!document.hidden) void load(); }, 10000);
    const focus = () => invalidate();
    window.addEventListener('focus', focus);
    return () => {
      alive = false; generation++; controller?.abort(); unsubscribe();
      window.clearInterval(timer); window.removeEventListener('focus', focus);
      refresh.current = () => {};
    };
  }, [agent, conversation, identity]);
  return { data: data?.agent === agent && data.conversation === conversation ? data : undefined,
    error, busy, refresh: () => refresh.current() };
}
