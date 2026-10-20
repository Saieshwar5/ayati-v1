import { useCallback, useEffect, useRef, useState } from 'react';

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : 'Request could not be completed.';
}

/** A synchronous gate prevents double submission before React commits the busy state. */
export function useMutation() {
  const pending = useRef(false);
  const alive = useRef(false);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState('');
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const run = async (action: () => Promise<void>) => {
    if (pending.current) return;
    pending.current = true; setBusy(true); setStatus('');
    try { await action(); }
    catch (error) { if (alive.current) setStatus(errorMessage(error)); }
    finally { pending.current = false; if (alive.current) setBusy(false); }
  };
  return { busy, status, setStatus, run };
}

/** Ignore stale reads and late responses after the owning component unmounts. */
export function useResource<T>(load: () => Promise<T>) {
  const [data, setData] = useState<T>();
  const [error, setError] = useState('');
  const generation = useRef(0);
  const refresh = useCallback(async () => {
    const request = ++generation.current;
    try { const next = await load(); if (generation.current === request) { setData(next); setError(''); } }
    catch (failure) { if (generation.current === request) setError(errorMessage(failure)); throw failure; }
  }, [load]);
  useEffect(() => {
    void refresh().catch(() => {});
    return () => { generation.current++; };
  }, [refresh]);
  return { data, setData, error, refresh };
}
