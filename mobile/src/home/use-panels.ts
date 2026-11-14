/**
 * The person's saved Home panels. Read when Home opens or comes back into
 * view, on pull to refresh, and again when access to panels changes. Viewing
 * starts no agent work (web rule).
 */
import { shownPanels, type Change } from '@cuckoo/shared';
import { useFocusEffect } from 'expo-router';
import { useCallback } from 'react';
import { useChanges } from '../live/live-context';
import { useWorkspace } from '../session/session-context';
import { useResource } from '../ui/requests';

export function usePanels() {
  const { api } = useWorkspace();
  const resource = useResource(
    useCallback(
      async (signal: AbortSignal) => {
        const page = await api.panels(signal);
        return { available: page.available, panels: shownPanels(page) };
      },
      [api],
    ),
  );
  const refresh = resource.refresh;
  useFocusEffect(
    useCallback(() => {
      void refresh();
    }, [refresh]),
  );
  useChanges(
    useCallback(
      (change: Change) => {
        if (['access', 'reset', 'panel_access', 'event_access'].includes(change.kind)) void refresh();
      },
      [refresh],
    ),
  );
  return resource;
}
