import { useEffect, useState } from 'react';
import { errorMessage } from '../requests';
import { denied, query, type ResultSource, type ResponseReference, type SavedResponse } from './model';

type Reading = {
  id: string; source: ResultSource; data?: SavedResponse; latest?: ResponseReference;
  error: string; busy: boolean;
};

/** Shared saved-body lifecycle for inline replies and the full reading dialog. */
export function useSavedResponse(id: string, source: ResultSource) {
  const [reading, setReading] = useState<Reading>();
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    const abort = new AbortController();
    let loading = false, epoch = 0, sourceScope: string | null | undefined;
    const empty = (): Reading => ({ id, source, error: '', busy: true });
    setReading(empty());
    const load = async (metadata: boolean) => {
      if (loading || abort.signal.aborted) return;
      loading = true; const request = ++epoch;
      try {
        const next = await source.read<SavedResponse>(query(source.response(id), { metadata }), abort.signal);
        if (abort.signal.aborted || request !== epoch) return;
        sourceScope = next.source_scope;
        setReading(previous => ({ ...empty(), data: metadata ? previous?.data : next, latest: next, busy: false }));
      } catch (failure) {
        if (abort.signal.aborted || request !== epoch) return;
        setReading(previous => ({ ...empty(), data: denied(failure) ? undefined : previous?.data,
          latest: denied(failure) ? undefined : previous?.latest, error: errorMessage(failure), busy: false }));
      } finally { if (request === epoch) loading = false; }
    };
    void load(false);
    const check = (scope?: string | null) => {
      if (scope !== undefined) {
        if (scope !== null && sourceScope !== undefined && sourceScope !== null && scope !== sourceScope) return;
        if (scope !== null && sourceScope === null) return;
        epoch++; loading = false; setReading(empty()); void load(false);
      } else void load(true);
    };
    const timer = window.setInterval(() => { if (!document.hidden) check(); }, 5000);
    const unsubscribe = source.subscribe?.(check);
    return () => { abort.abort(); epoch++; window.clearInterval(timer); unsubscribe?.(); };
  }, [id, source, retry]);
  const current = reading?.id === id && reading.source === source ? reading : undefined;
  return { data: current?.data, latest: current?.latest, error: current?.error ?? '',
    busy: current?.busy ?? true, update: () => setRetry(value => value + 1) };
}
