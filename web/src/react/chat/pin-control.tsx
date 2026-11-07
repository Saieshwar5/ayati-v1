import { useRef } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { chatRequest } from './model';
import { samePin, type Pin, type Preferences } from './organization-model';
export function PinButton({ pin, name, preferences, busy, toggle }: { pin: Pin; name: string; preferences?: Preferences; busy: boolean; toggle: (pin: Pin) => void }) {
  if (!preferences?.available) return null;
  const pinned = preferences.pins.some(item => samePin(item, pin));
  return <Button className="chat-pin-action" aria-label={`${pinned ? 'Unpin' : 'Pin'} ${name}`} title={pinned ? 'Unpin conversation' : 'Pin conversation'} aria-pressed={pinned} disabled={busy} onClick={() => toggle(pin)}>{pinned ? '★' : '☆'}</Button>;
}
/** The labelled Pin among a conversation's quiet actions in Details. */
export function PinAction({ pin, preferences, busy, toggle }: { pin: Pin; preferences?: Preferences; busy: boolean; toggle: (pin: Pin) => void }) {
  if (!preferences?.available) return null;
  const pinned = preferences.pins.some(item => samePin(item, pin));
  return <Button className="chat-details-action" aria-pressed={pinned} title={pinned ? 'Unpin conversation' : 'Pin conversation'} disabled={busy} onClick={() => toggle(pin)}>
    <svg viewBox="0 0 24 24" width="15" height="15" fill={pinned ? 'currentColor' : 'none'} stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><path d="M9 4h6l-1 6 3 3v1H7v-1l3-3-1-6ZM12 14v6" /></svg>
    Pin
  </Button>;
}
export function usePinActions(preferences: Preferences | undefined, saved: (next: Preferences) => void, epoch: { current: number }, refresh: () => Promise<void>) {
  const mutation = useMutation();
  const attempt = useRef<{ signature: string; request: string }>(undefined);
  const toggle = (pin: Pin) => {
    if (!preferences?.available) return;
    const pins = preferences.pins.some(item => samePin(item, pin)) ? preferences.pins.filter(item => !samePin(item, pin)) : [...preferences.pins, pin];
    const payload = { revision: preferences.revision, pins };
    void mutation.run(async () => {
      const signature = JSON.stringify(payload);
      if (attempt.current?.signature !== signature) attempt.current = { signature, request: crypto.randomUUID() };
      const request = attempt.current.request, source = epoch.current;
      const next = await chatRequest<Preferences>('/chat-pins', { ...payload, request });
      if (epoch.current !== source) { await refresh(); return; }
      saved(next); attempt.current = undefined;
    });
  };
  return { ...mutation, toggle };
}
