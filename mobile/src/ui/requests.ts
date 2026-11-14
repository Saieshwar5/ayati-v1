/**
 * The web's request rules (web/src/react/requests.ts) for the phone: a
 * synchronous gate against double submission, and reads that ignore answers
 * arriving after a newer read or after the screen has gone.
 */
import { useCallback, useEffect, useRef, useState } from 'react';

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : 'Request could not be completed.';
}

export function useMutation() {
  const pending = useRef(false);
  const alive = useRef(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  const run = useCallback(async (action: () => Promise<void>) => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError('');
    try {
      await action();
    } catch (failure) {
      if (alive.current) setError(errorMessage(failure));
    } finally {
      pending.current = false;
      if (alive.current) setBusy(false);
    }
  }, []);
  return { busy, error, setError, run };
}

/** Load on mount and on `refresh`; a newer read always wins. */
export function useResource<T>(load: (signal: AbortSignal) => Promise<T>) {
  const [data, setData] = useState<T>();
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(true);
  const current = useRef<AbortController | undefined>(undefined);
  const refresh = useCallback(async () => {
    current.current?.abort();
    const abort = new AbortController();
    current.current = abort;
    setLoading(true);
    try {
      const next = await load(abort.signal);
      if (!abort.signal.aborted) {
        setData(next);
        setError('');
      }
    } catch (failure) {
      if (!abort.signal.aborted) setError(errorMessage(failure));
    } finally {
      if (!abort.signal.aborted) setLoading(false);
    }
  }, [load]);
  useEffect(() => {
    void refresh();
    return () => current.current?.abort();
  }, [refresh]);
  return { data, setData, error, loading, refresh };
}
