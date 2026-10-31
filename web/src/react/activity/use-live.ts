import { useEffect } from 'react';
import { useResource } from '../requests';
import { subscribeChanges } from '../chat/live';

/** A read that follows live changes: messages and progress refresh it within a
 * second; a slower poll stays as the fallback. Access changes clear it first. */
export function useLive<T>(load: () => Promise<T>, poll = 10000) {
  const resource = useResource(load);
  const { refresh, setData } = resource;
  useEffect(() => {
    let timer: number | undefined;
    const soon = () => { window.clearTimeout(timer); timer = window.setTimeout(() => { if (!document.hidden) void refresh().catch(() => {}); }, 800); };
    const unsubscribe = subscribeChanges(change => {
      if (change.kind === 'access' || change.kind === 'event_access') { setData(undefined); soon(); }
      else if (change.kind === 'message' || change.kind === 'reset') soon();
    });
    const interval = window.setInterval(() => { if (!document.hidden) void refresh().catch(() => {}); }, poll);
    return () => { unsubscribe(); window.clearInterval(interval); window.clearTimeout(timer); };
  }, [refresh, setData, poll]);
  return resource;
}
