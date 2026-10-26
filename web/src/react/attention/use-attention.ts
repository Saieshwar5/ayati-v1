import { useCallback, useEffect, useRef, useState } from 'react';
import { subscribeChanges } from '../chat/live';
import { errorMessage } from '../requests';
import { loadAttention, type AttentionPage, type AttentionView } from './model';

/** One visible page, with invalidation of old reads on source/identity changes. */
export function useAttention(view: AttentionView, before: string | null, enabled = true) {
  const key = JSON.stringify([view, before]);
  const [snapshot, setSnapshot] = useState<{ key: string; data: AttentionPage }>();
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const generation = useRef(0), controller = useRef<AbortController | null>(null);
  const busy = useRef(false), active = useRef(false);
  const refresh = useCallback(async () => {
    if (!active.current || busy.current) return;
    busy.current = true;
    const request = ++generation.current;
    const abort = new AbortController(); controller.current = abort;
    setLoading(true);
    try {
      const next = await loadAttention(view, before, abort.signal);
      if (generation.current === request && active.current) { setSnapshot({ key, data: next }); setError(''); }
    } catch (failure) {
      if (generation.current === request && active.current && !abort.signal.aborted) {
        setError(errorMessage(failure));
        // The inbox is a convenience projection. Clear titles on any failed read
        // rather than presenting a stale private queue as currently accessible.
        setSnapshot(undefined);
      }
    } finally {
      if (generation.current === request) { busy.current = false; setLoading(false); }
    }
  }, [view, before, key]);
  useEffect(() => {
    active.current = enabled;
    setError('');
    const invalidate = () => {
      generation.current++; controller.current?.abort(); busy.current = false;
      setSnapshot(undefined); if (active.current) void refresh();
    };
    if (enabled) void refresh();
    const timer = enabled ? window.setInterval(() => { if (!document.hidden) void refresh(); }, 5000) : undefined;
    const unsubscribe = subscribeChanges(change => {
      if (change.kind === 'access' || change.kind === 'reset') invalidate();
      // The periodic read coalesces progress/message bursts. Source revocation
      // uses the existing access event immediately, without a second SSE stream.
    });
    const focus = () => void refresh();
    if (enabled) window.addEventListener('focus', focus);
    return () => {
      active.current = false; generation.current++; controller.current?.abort(); busy.current = false;
      window.clearInterval(timer); unsubscribe(); window.removeEventListener('focus', focus);
    };
  }, [refresh, enabled]);
  return { data: snapshot?.key === key ? snapshot.data : undefined, error, loading, refresh };
}
