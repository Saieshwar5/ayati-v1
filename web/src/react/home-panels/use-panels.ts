import { useEffect, useRef, useState } from 'react';
import { storageScope } from '../../workspace-context';
import { errorMessage } from '../requests';
import { subscribeChanges } from '../chat/live';
import { readPanels, type PanelsPage } from './model';
export function usePanels() {
  const [data, setData] = useState<PanelsPage>();
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(true);
  const refresh = useRef<() => Promise<void>>(async () => {});
  const identity = storageScope('home-panels');
  useEffect(() => {
    let alive = true,
      loading = false,
      epoch = 0;
    let controller: AbortController | undefined;
    setData(undefined);
    setError('');
    setBusy(true);
    const load = async () => {
      if (!alive || loading) return;
      loading = true;
      setBusy(true);
      const request = ++epoch;
      controller = new AbortController();
      const signal = controller.signal;
      try {
        const next = await readPanels(signal);
        if (alive && !signal.aborted && request === epoch) {
          setData(next);
          setError('');
        }
      } catch (failure) {
        if (alive && !signal.aborted && request === epoch) {
          setData(undefined);
          setError(errorMessage(failure));
        }
      } finally {
        if (alive && request === epoch) {
          loading = false;
          setBusy(false);
        }
      }
    };
    const invalidate = () => {
      epoch++;
      controller?.abort();
      loading = false;
      setData(undefined);
      void load();
    };
    refresh.current = load;
    void load();
    const unsubscribe = subscribeChanges((change) => {
      if (['access', 'reset', 'event_access', 'panel_access'].includes(change.kind)) invalidate();
    });
    const timer = window.setInterval(() => {
      if (!document.hidden) void load();
    }, 5000);
    const focus = () => void load();
    window.addEventListener('focus', focus);
    return () => {
      alive = false;
      epoch++;
      controller?.abort();
      unsubscribe();
      window.clearInterval(timer);
      window.removeEventListener('focus', focus);
      refresh.current = async () => {};
    };
  }, [identity]);
  return { data, error, busy, refresh: () => refresh.current() };
}
