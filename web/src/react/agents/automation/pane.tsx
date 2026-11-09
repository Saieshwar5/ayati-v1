import { useCallback, useEffect, useRef, useState } from 'react';
import type { Agent } from '../model';
import { Button } from '../../ui';
import { useResource } from '../../requests';
import { subscribeChanges } from '../../chat/live';
import { CreateSchedule } from './create-dialog';
import { ScheduleState } from './state-dialog';
import { ScheduleRuns } from './runs-dialog';
import { cadence, date, deviceClock, loadSchedules, results, stateLabels, type Routine } from './model';
import './schedules.css';
import { EventTriggers } from './events';

type Open = { kind: 'create' } | { kind: 'runs'; routine: Routine } | { kind: 'state'; routine: Routine; enabled?: boolean };
export function AutomationPane({ agent }: { agent: Agent }) {
  return agent.editable ? <ScheduleWorkspace key={agent.id} agent={agent} /> : <><h3>Automations</h3><p>Only this agent's managers can manage its schedules and triggers. Individual work remains visible through your authorized conversations.</p></>;
}
function ScheduleWorkspace({ agent }: { agent: Agent }) {
  const load = useCallback(() => loadSchedules(agent.id), [agent.id]);
  const resource = useResource(load);
  const [clock] = useState(deviceClock);
  const [open, setOpen] = useState<Open>();
  const opener = useRef<HTMLElement | null>(null), refreshButton = useRef<HTMLButtonElement | null>(null);
  const focusTimer = useRef<number | undefined>(undefined);
  const show = (next: Open) => { opener.current = document.activeElement as HTMLElement | null; setOpen(next); };
  const close = () => {
    setOpen(undefined);
    focusTimer.current = window.setTimeout(() => {
      if (!opener.current?.isConnected) refreshButton.current?.focus();
    }, 0);
  };
  const polling = useRef(false);
  useEffect(() => {
    const refresh = async () => {
      if (polling.current || document.hidden) return;
      polling.current = true;
      try { await resource.refresh(); } catch { /* Resource exposes the failure. */ }
      finally { polling.current = false; }
    };
    const timer = window.setInterval(() => void refresh(), 5000);
    const access = subscribeChanges(change => {
      if (change.kind !== 'access' && change.kind !== 'reset') return;
      setOpen(undefined); resource.setData(undefined); void resource.refresh().catch(() => {});
    });
    return () => { clearInterval(timer); window.clearTimeout(focusTimer.current); access(); };
  }, [resource.refresh, resource.setData]);
  const ready = Boolean(resource.data && !resource.error);
  return <section className="shared-schedules" aria-label={`${agent.name} automations`}>
    <header className="agent-section-heading"><div><h3>Automations</h3>
      <p>Work {agent.name} does on its own. Every run follows its tool rules, and Ask-first actions still wait for a person. Runs and anything that needs you appear in Activity.</p></div>
    </header>
    <section className="automation-group" aria-labelledby={`${agent.id}-on-schedule`}>
      <header className="agent-section-heading"><div><h4 id={`${agent.id}-on-schedule`}>On a schedule</h4><p>At a time, once or repeating.</p></div>
        <Button disabled={!ready || !resource.data?.execution_available} onClick={() => show({ kind: 'create' })}>New schedule</Button>
      </header>
      {resource.error && <p role="alert">{resource.error}</p>}
      {!resource.data && !resource.error && <p role="status">Loading schedules…</p>}
      {resource.data?.execution_available === false && <p role="status">Schedules need pooled background work, which this server doesn’t run. Saved schedules stay readable; they can’t run here.</p>}
      {resource.data?.schedules.map(routine => <article className="routine-row" key={routine.id} aria-label={`Schedule: ${routine.name}`}>
        <header><h4>{routine.name}</h4><span>{stateLabels[routine.state]}</span></header>
        <p className="routine-instructions">{routine.content}{routine.instructions_long && '…'}</p>
        <p>{cadence(routine.interval_seconds)} · {routine.timezone}</p>
        <p>Results: {results(routine.chat, routine.chat_name)} · Set up by {routine.principal_name}</p>
        {routine.next_ms && <p>Next run: {date(routine.next_ms, routine.timezone)}</p>}
        {routine.unfinished && <p className="ui-help">An earlier run is still unfinished. View runs to check its progress or use task Stop.</p>}
        {routine.detail && <p role="status">{routine.detail}</p>}
        <div className="routine-actions"><Button onClick={() => show({ kind: 'state', routine })}>View instructions</Button><Button onClick={() => show({ kind: 'runs', routine })}>View runs</Button>
          {routine.state === 'enabled' && <Button disabled={!ready} onClick={() => show({ kind: 'state', routine, enabled: false })}>Pause</Button>}
          {routine.state !== 'enabled' && routine.state !== 'completed' && <Button disabled={!ready || !routine.can_resume || !resource.data?.execution_available} onClick={() => show({ kind: 'state', routine, enabled: true })}>Review and resume</Button>}
        </div>
        {!routine.can_resume && routine.state !== 'enabled' && routine.state !== 'completed' && <p className="ui-help">Only {routine.principal_name} can resume it, with their current access.</p>}
      </article>)}
      {resource.data && !resource.data.schedules.length && <p className="ui-help">No schedules yet.</p>}
      <div className="automation-tools"><Button ref={refreshButton} onClick={() => void resource.refresh().catch(() => {})}>Refresh schedules</Button></div>
    </section>
    <EventTriggers agent={agent.id}/>
    {open?.kind === 'create' && resource.data && <CreateSchedule agent={agent.id} data={resource.data} clock={clock} close={close} saved={resource.refresh} />}
    {open?.kind === 'runs' && <ScheduleRuns agent={agent.id} routine={open.routine} close={close} />}
    {open?.kind === 'state' && <ScheduleState agent={agent.id} routine={open.routine} enabled={open.enabled} close={close} saved={resource.refresh} />}
  </section>;
}
