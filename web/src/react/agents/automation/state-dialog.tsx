import { useCallback, useEffect, useRef, useState } from 'react';
import { Button, Dialog } from '../../ui';
import { useMutation, useResource } from '../../requests';
import { cadence, changeSchedule, date, loadSchedule, type Routine } from './model';
export function ScheduleState({ agent, routine, enabled, close, saved }: { agent: string; routine: Routine; enabled?: boolean; close: () => void; saved: () => Promise<void> }) {
  const load = useCallback(() => loadSchedule(agent, routine.id), [agent, routine.id]);
  const resource = useResource(load);
  const attempt = useRef<{ request: string; revision: number; enabled: boolean } | undefined>(undefined);
  const mutation = useMutation();
  const alive = useRef(false);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const [confirmed, setConfirmed] = useState(false);
  const label = enabled === undefined ? 'Routine instructions' : enabled ? 'Resume routine' : 'Pause routine';
  const data = resource.data;
  const footer = confirmed || enabled === undefined ? <Button onClick={close}>Done</Button> : <Button primary disabled={mutation.busy || !data || Boolean(resource.error) || (enabled && !data.can_resume)} onClick={() => void mutation.run(async () => {
    if (!data) return;
    attempt.current ??= { request: crypto.randomUUID(), revision: data.revision, enabled };
    await changeSchedule(agent, routine.id, attempt.current);
    if (!alive.current) return;
    setConfirmed(true);
    try { await saved(); } catch { throw new Error('Routine state saved, but its list could not refresh. Close this review and refresh schedules.'); }
  })}>{label}</Button>;
  return <Dialog title={label} close={close} busy={mutation.busy} status={mutation.status} footer={footer}>
    {resource.error && <p role="alert">{resource.error}<Button onClick={() => void resource.refresh().catch(() => {})}>Retry routine details</Button></p>}
    {!data && !resource.error && <p role="status">Checking saved instructions and current access…</p>}
    {confirmed ? <p role="status">Routine state saved. Refresh its runs to inspect individual task outcomes.</p> : data && <>
      <h3>{data.name}</h3><p className="routine-instructions">{data.content}</p>
      <p>{data.chat_name} · {cadence(data.interval_seconds)} · {data.timezone}</p>
      <p>Authorized by {data.principal_name}.</p>
      {enabled === true && <><p>Review current agent and destination access before resuming. Missing intervals coalesce into one latest due run; a still-unfinished run prevents overlap.</p>
        {data.next_ms && <p>Saved next due time: {date(data.next_ms, data.timezone)}.</p>}
        <p className="ui-help">Future runs can use the configured model while you are offline. Normal business approvals still apply.</p></>}
      {enabled === false && <p>Pause prevents future runs and cancels initial work that has not started. Running tasks and saved question/approval continuations keep their own Stop control in Chat or Work.</p>}
    </>}
  </Dialog>;
}
