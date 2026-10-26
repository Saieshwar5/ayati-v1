import { useEffect, useRef, useState } from 'react';
import { storageScope } from '../../workspace-context';
import { errorMessage } from '../requests';
import { denied } from '../results/model';
import type { AgentRequest } from '../agents/request-model';
import type { QuestionReference } from '../questions/model';
import { chatRequest } from './model';
import { subscribeChanges } from './live';
export type TaskProgressRecord = AgentRequest & {
  checked_ms: number;
  worker_released: boolean;
  questions: QuestionReference[];
};
export function useTaskProgress(chat: string, reply: string) {
  const [data, setData] = useState<TaskProgressRecord>();
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(true);
  const refresh = useRef<() => void>(() => {});
  const identity = storageScope('task-progress');
  useEffect(() => {
    let stopped = false,
      loading = false,
      epoch = 0,
      scope: string | null | undefined;
    let controller: AbortController | undefined;
    setData(undefined);
    setError('');
    setBusy(true);
    const load = () => {
      if (stopped || loading) return;
      loading = true;
      setBusy(true);
      const request = ++epoch;
      controller = new AbortController();
      const signal = controller.signal;
      void chatRequest<TaskProgressRecord>(
        `/chats/${encodeURIComponent(chat)}/messages/${encodeURIComponent(reply)}/progress`,
        undefined,
        signal,
      )
        .then((next) => {
          if (stopped || signal.aborted || request !== epoch) return;
          scope = next.source_scope;
          setData(next);
          setError('');
        })
        .catch((failure) => {
          if (stopped || signal.aborted || request !== epoch) return;
          if (denied(failure)) setData(undefined);
          setError(errorMessage(failure));
        })
        .finally(() => {
          if (!stopped && request === epoch) {
            loading = false;
            setBusy(false);
          }
        });
    };
    const invalidate = () => {
      epoch++;
      controller?.abort();
      loading = false;
      setData(undefined);
      setError('');
      load();
    };
    refresh.current = load;
    load();
    const unsubscribe = subscribeChanges((change) => {
      if (change.kind === 'access' || change.kind === 'reset') invalidate();
      else if (change.kind === 'event_access' && change.chat === chat) {
        if (scope === null || (scope !== undefined && change.source && scope !== change.source)) return;
        invalidate();
      } else if (change.chat === chat && change.message?.id === reply) load();
    });
    const timer = window.setInterval(() => {
      if (!document.hidden) load();
    }, 5000);
    return () => {
      stopped = true;
      epoch++;
      controller?.abort();
      unsubscribe();
      window.clearInterval(timer);
      refresh.current = () => {};
    };
  }, [chat, reply, identity]);
  return { data, error, busy, refresh: () => refresh.current() };
}
