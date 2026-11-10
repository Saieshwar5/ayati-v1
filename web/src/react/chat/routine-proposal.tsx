import { useRef, useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { identityRequest, workspaceId, workspaceSession, workspaceUrl } from '../../workspace-context';

/** "Every week", "Every 2 days", "Every 3 hours". */
export function every(seconds: number): string {
  if (seconds % 604800 === 0) return seconds === 604800 ? 'Every week' : `Every ${seconds / 604800} weeks`;
  if (seconds % 86400 === 0) return seconds === 86400 ? 'Every day' : `Every ${seconds / 86400} days`;
  if (seconds % 3600 === 0) return seconds === 3600 ? 'Every hour' : `Every ${seconds / 3600} hours`;
  return `Every ${Math.round(seconds / 60)} minutes`;
}

export type RoutineProposal = { id: string; name: string; content?: string; first_ms: number; interval_seconds: number | null; state: 'pending' | 'saved' | 'dismissed'; requester: string };

/** A schedule the agent proposed (Phase 4 T3): saved only when the person who asked
 * says so, and then it runs as them. Others see that it was proposed. */
export function RoutineCard({ routine, agent }: { routine: RoutineProposal; agent: string }) {
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
  return <section className="routine-card" aria-label={`Schedule: ${routine.name}`}>
    <strong>{state === 'saved' ? 'Schedule saved' : 'Schedule'} · {routine.name}</strong>
    {state === 'pending' && <small>{agent} suggests this. Nothing runs until {mine ? 'you save' : 'the person who asked saves'} it.</small>}
    <dl className="routine-card-facts">
      <dt>When</dt><dd>{routine.interval_seconds ? `${every(routine.interval_seconds)}, first ${when}` : `Once, ${when}`}</dd>
      {routine.content && <><dt>{agent} will</dt><dd>{routine.content}</dd></>}
    </dl>
    {state === 'pending' && mine && <span className="routine-card-actions">
      <Button primary disabled={m.busy} onClick={() => decide(true)}>Save schedule</Button>
      <Button disabled={m.busy} onClick={() => decide(false)}>Not now</Button></span>}
    {state === 'pending' && !mine && <small>Waiting for the person who asked to save it.</small>}
    {state === 'saved' && <small>It runs as {mine ? 'you' : 'the person who saved it'}, with {mine ? 'your' : 'their'} access; Ask-first actions still wait for approval.{' '}
      <a href={workspaceUrl('/?view=activity&tab=upcoming')}>See it in Activity › Upcoming</a></small>}
    {state === 'dismissed' && <small>Not saved.</small>}
    {m.status && <small role="alert">{m.status}</small>}
  </section>;
}
