import { useEffect } from 'react';
import { chatRequest } from '../chat/model';
import { subscribeChanges } from '../chat/live';
import { setNotifyDefaults } from '../chat/notifications';
import { refreshUnread } from '../chat/unread-counts';
import { useMutation, useResource } from '../requests';

/** `show_online` is "Show when I'm online" (D245); the server keeps it with your presence. */
export type SavedPreferences = { desktop_alerts: boolean; channel_level: string; approval_alerts: boolean; show_online?: boolean };
const loadPreferences = () => chatRequest<SavedPreferences>('/preferences');

/** Your notification defaults and presence choice, kept on the server so they follow
 * you. Each change saves at once; Preferences and Profile both edit them. */
export function usePreferences() {
  const resource = useResource(loadPreferences);
  const mutation = useMutation();
  // Changed on another device (schema 140): show what is saved now.
  const { refresh } = resource;
  useEffect(() => subscribeChanges(change => { if (change.kind === 'levels') void refresh().catch(() => {}); }), [refresh]);
  const save = (change: Partial<SavedPreferences>) => {
    const data = resource.data;
    if (!data) return;
    const next = { ...data, ...change };
    void mutation.run(async () => {
      resource.setData(await chatRequest<SavedPreferences>('/preferences', next));
      setNotifyDefaults(next); refreshUnread(); mutation.setStatus('Saved.');
    });
  };
  return { data: resource.data, error: resource.error, busy: mutation.busy, status: mutation.status, save };
}
export type Preferences = ReturnType<typeof usePreferences>;
