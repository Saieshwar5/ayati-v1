/**
 * One live stream per open workspace. Screens subscribe to the changes they
 * care about; only the connection banner reads the status, so a status change
 * re-renders nothing else.
 */
import type { Change } from '@cuckoo/shared';
import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import { AppState } from 'react-native';
import { useSession, useWorkspace } from '../session/session-context';
import { startLive, type LiveStatus } from './stream';

type Listener = (change: Change) => void;
type Changes = { subscribe: (listener: Listener) => () => void };

const ChangesContext = createContext<Changes | undefined>(undefined);
const StatusContext = createContext<LiveStatus>('connecting');

export function LiveProvider({ children }: { children: ReactNode }) {
  const { api } = useWorkspace();
  const { client, refresh, token } = useSession();
  const [status, setStatus] = useState<LiveStatus>('connecting');
  const listeners = useMemo(() => new Set<Listener>(), []);
  const changes = useMemo<Changes>(
    () => ({
      subscribe: listener => {
        listeners.add(listener);
        return () => {
          listeners.delete(listener);
        };
      },
    }),
    [listeners],
  );

  useEffect(() => {
    const live = startLive({
      url: api.liveUrl,
      token,
      fetch: (input, init) => fetch(input, init),
      onStatus: setStatus,
      // A 401 on the stream means the device session ended: the session check signs out.
      onUnauthorized: () => void refresh().catch(() => {}),
      onBatch: batch => {
        const all: Change[] = batch.reset ? [{ kind: 'reset' }, ...batch.events] : batch.events;
        for (const change of all) listeners.forEach(listener => listener(change));
      },
    });
    const appState = AppState.addEventListener('change', next => (next === 'active' ? live.resume() : live.pause()));
    return () => {
      appState.remove();
      live.stop();
    };
  }, [api, token, client, refresh, listeners]);

  return (
    <ChangesContext.Provider value={changes}>
      <StatusContext.Provider value={status}>{children}</StatusContext.Provider>
    </ChangesContext.Provider>
  );
}

/** Call `listener` for each live change while the screen is mounted. */
export function useChanges(listener: Listener) {
  const changes = useContext(ChangesContext);
  useEffect(() => changes?.subscribe(listener), [changes, listener]);
}

export const useLiveStatus = () => useContext(StatusContext);
