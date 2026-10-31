import { useCallback, useEffect, useRef, useState } from 'react';
import { currentWorkspace, identityRequest, workspaceId } from '../../workspace-context';
import { subscribeChanges } from '../chat/live';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';

/** Whether all agents are paused (an admin's emergency switch). */
export type PauseState = { paused: boolean; since_ms: number | null; by: string | null; note: string | null; active: number | null };

const base = `/workspaces/${workspaceId}/agents`;
const loadState = (signal?: AbortSignal) => identityRequest<PauseState>(`${base}/pause`, undefined, signal);
const pause = (note: string) => identityRequest<PauseState>(`${base}/pause`, { note: note.trim() || null });
const resume = () => identityRequest<PauseState>(`${base}/resume`, {});
const admin = () => currentWorkspace()?.role === 'Admin';
const when = (ms: number) => new Date(ms).toLocaleString([], { weekday: 'short', hour: 'numeric', minute: '2-digit' });

/** The pause state, kept current by the live change stream rather than polling.
 * It reads the server only when the session says agents are paused or a pause or
 * resume happens; otherwise it costs nothing. */
export function usePauseState(always = false) {
  const initial = currentWorkspace()?.agents_paused_ms ?? null;
  const [state, setState] = useState<PauseState | undefined>(initial || always ? undefined : { paused: false, since_ms: null, by: null, note: null, active: null });
  // Reconnects and the polling fallback send "reset"; re-read on those only when
  // a pause is showing (or on admin screens), so ordinary pages stay quiet.
  const showing = useRef(Boolean(initial) || always);
  useEffect(() => { showing.current = always || Boolean(state?.paused); }, [always, state]);
  const refresh = useCallback(async (signal?: AbortSignal) => {
    try {
      const next = await loadState(signal);
      if (signal?.aborted) return;
      setState(next);
      // Later screens in this tab start from what we know now.
      const entry = currentWorkspace(); if (entry) entry.agents_paused_ms = next.since_ms;
    } catch { /* Keep the last known state; the next change retries. */ }
  }, []);
  useEffect(() => {
    const abort = new AbortController();
    if (initial || always) void refresh(abort.signal);
    const unsubscribe = subscribeChanges(change => {
      if (change.kind === 'agents' || (change.kind === 'reset' && showing.current)) void refresh(abort.signal);
    });
    return () => { abort.abort(); unsubscribe(); };
  }, [initial, always, refresh]);
  return { state, setState, refresh };
}

/** Shown on every page while agents are paused; admins can resume from it. */
export function PauseBanner() {
  const { state, setState } = usePauseState();
  const mutation = useMutation();
  if (!state?.paused) return null;
  return <div className="agents-paused-banner" role="status">
    <span><strong>All agents are paused</strong>{state.by ? ` by ${state.by}` : ''}{state.since_ms ? ` since ${when(state.since_ms)}` : ''}
      {state.note ? ` · “${state.note}”` : ''}. No agent takes new work until an admin resumes.</span>
    {admin() && <Button disabled={mutation.busy} onClick={() => void mutation.run(async () => setState(await resume()))}>{mutation.busy ? 'Resuming…' : 'Resume agents'}</Button>}
    {mutation.status && <span role="alert">{mutation.status}</span>}
  </div>;
}

/** Pause all agents / Resume agents, for admins (Activity → Now, Settings → Workspace). */
export function PauseAgentsControl({ changed }: { changed?: () => void }) {
  const { state, setState } = usePauseState(true);
  const [confirming, setConfirming] = useState(false);
  const [note, setNote] = useState('');
  const mutation = useMutation();
  if (!admin() || !state) return null;
  const done = (next: PauseState) => { setState(next); changed?.(); };
  if (state.paused) return <Button disabled={mutation.busy} onClick={() => void mutation.run(async () => done(await resume()))}>
    {mutation.busy ? 'Resuming…' : 'Resume agents'}</Button>;
  const stopping = state.active ?? 0;
  const close = () => { setConfirming(false); setNote(''); mutation.setStatus(''); };
  return <>
    <Button className="danger" onClick={() => setConfirming(true)}>Pause all agents</Button>
    {confirming && <Dialog title="Pause all agents?" busy={mutation.busy} status={mutation.status} close={close}
      footer={<><Button disabled={mutation.busy} onClick={close}>Cancel</Button>
        <Button className="danger" disabled={mutation.busy} onClick={() => void mutation.run(async () => { done(await pause(note)); close(); })}>
          {mutation.busy ? 'Pausing…' : 'Pause all agents'}</Button></>}>
      <p>Every agent stops at once. {stopping === 0 ? 'No requests are running or waiting right now.'
        : `${stopping} ${stopping === 1 ? 'request that is' : 'requests that are'} running or waiting will be stopped and won’t restart on their own.`}</p>
      <p>Until an admin resumes, no agent takes new requests, schedules skip their runs and events from connected tools are not handled. Agents paused on their own stay paused after you resume.</p>
      <label className="ui-field">Reason <small>everyone in the workspace sees it</small>
        <input maxLength={300} value={note} placeholder="For example: wrong emails going out" onChange={event => setNote(event.target.value)} /></label>
    </Dialog>}
  </>;
}
