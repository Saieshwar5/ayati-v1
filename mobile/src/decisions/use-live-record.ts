/**
 * A record that can change while its sheet is open (an approval or a
 * question): read on open, every few seconds while the app is in front, and
 * at once after an access change. Each read replaces the last; a newer read
 * always wins.
 */
import type { Change } from '@cuckoo/shared';
import { useCallback, useEffect } from 'react';
import { AppState } from 'react-native';
import { useChanges } from '../live/live-context';
import { useResource } from '../ui/requests';

export function useLiveRecord<T>(load: (signal: AbortSignal) => Promise<T>, everyMs: number) {
  const resource = useResource(load);
  const { refresh, setData } = resource;
  useEffect(() => {
    const timer = setInterval(() => {
      if (AppState.currentState === 'active') void refresh();
    }, everyMs);
    return () => clearInterval(timer);
  }, [refresh, everyMs]);
  useChanges(
    useCallback(
      (change: Change) => {
        if (change.kind === 'access' || change.kind === 'reset') {
          // Access may have ended: never keep showing details someone lost.
          setData(undefined);
          void refresh();
        }
      },
      [refresh, setData],
    ),
  );
  return resource;
}
