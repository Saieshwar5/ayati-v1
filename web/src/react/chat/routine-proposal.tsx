import { useRef, useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { identityRequest, workspaceId, workspaceSession } from '../../workspace-context';

/** "Every week", "Every 2 days", "Every 3 hours". */
export function every(seconds: number): string {
  if (seconds % 604800 === 0) return seconds === 604800 ? 'Every week' : `Every ${seconds / 604800} weeks`;
  if (seconds % 86400 === 0) return seconds === 86400 ? 'Every day' : `Every ${seconds / 86400} days`;
  if (seconds % 3600 === 0) return seconds === 3600 ? 'Every hour' : `Every ${seconds / 3600} hours`;
  return `Every ${Math.round(seconds / 60)} minutes`;
}

export type RoutineProposal = { id: string; name: string; first_ms: number; interval_seconds: number | null; state: 'pending' | 'saved' | 'dismissed'; requester: string };

/** A routine the agent proposed (Phase 4 T3): saved only when the person who asked
 * says so. Others see that it was proposed. */
export function RoutineCard({ routine }: { routine: RoutineProposal }) {
  const [state, setState] = useState(routine.state);
  const m = useMutation();
  const sent = useRef(false);
  const mine = workspaceSession?.user?.id === routine.requester;
  const when = new Date(routine.first_ms).toLocaleString([], { weekday: 'short', day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
  const decide = (save: boolean) => {
    if (sent.current) return;
    sent.current = true;
    void m.run(async () => {
      try {
        const result = await identityRequest<{ state: RoutineProposal['state'] }>(`/workspaces/${workspaceId}/activity/routine-proposals/${encodeURIComponent(routine.id)}`, { save });
        setState(result.state);
      } finally { sent.current = false; }
    });
  };
  return <section className="routine-card" aria-label={`Routine proposal: ${routine.name}`}>
    <strong>Routine: {routine.name}</strong>
    <small>{routine.interval_seconds ? `${every(routine.interval_seconds)}, first ${when}` : `Once, ${when}`}</small>
    {state === 'pending' && mine && <span className="routine-card-actions">
      <Button primary disabled={m.busy} onClick={() => decide(true)}>Save routine</Button>
      <Button disabled={m.busy} onClick={() => decide(false)}>Not now</Button></span>}
    {state === 'pending' && !mine && <small>Waiting for the person who asked to save it.</small>}
    {state === 'saved' && <small>Saved. It runs on the agent's Schedules.</small>}
    {state === 'dismissed' && <small>Not saved.</small>}
    {m.status && <small role="alert">{m.status}</small>}
  </section>;
}
