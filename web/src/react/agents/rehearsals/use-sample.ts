import { useCallback, useEffect, useRef, useState } from 'react';
import { subscribeChanges } from '../../chat/live';
import { errorMessage } from '../../requests';
import { activeSample, loadSample, type Sample } from './model';

/** Personal sample status; never restarts inference when reading or reconnecting. */
export function useSample(agent: string, id: string) {
  const key = `${agent}/${id}`;
  const [snapshot, setSnapshot] = useState<{ key: string; data: Sample }>();
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const generation = useRef(0), busy = useRef(false), alive = useRef(false);
  const abort = useRef<AbortController | undefined>(undefined);
  const phase = useRef<string | undefined>(undefined);
  const refresh = useCallback(async () => {
    if (!id || !alive.current || busy.current) return;
    busy.current = true; const request = ++generation.current;
    const controller = new AbortController(); abort.current = controller; setLoading(true);
    try {
      const next = await loadSample(agent, id, controller.signal);
      if (alive.current && generation.current === request) { setSnapshot({ key, data: next }); phase.current = next.state; setError(''); }
    } catch (failure) {
      if (alive.current && generation.current === request && !controller.signal.aborted) { setSnapshot(undefined); setError(errorMessage(failure)); }
    } finally { if (alive.current && generation.current === request) { busy.current = false; setLoading(false); } }
  }, [agent, id, key]);
  useEffect(() => {
    alive.current = true; phase.current = undefined; setError('');
    if (!id) setSnapshot(undefined);
    void refresh();
    const timer = id ? window.setInterval(() => { if (!document.hidden && (!phase.current || activeSample(phase.current))) void refresh(); }, 2000) : undefined;
    const unsubscribe = subscribeChanges(change => {
      if (change.kind !== 'access' && change.kind !== 'reset' && change.kind !== 'sample_access') return;
      generation.current++; abort.current?.abort(); busy.current = false; setSnapshot(undefined); void refresh();
    });
    const focus = () => void refresh(); window.addEventListener('focus', focus);
    return () => { alive.current = false; generation.current++; abort.current?.abort(); busy.current = false; clearInterval(timer); unsubscribe(); window.removeEventListener('focus', focus); };
  }, [refresh, id]);
  return { data: snapshot?.key === key ? snapshot.data : undefined, error, loading, refresh };
}
