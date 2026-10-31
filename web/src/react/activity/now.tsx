import { useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { AgentFace } from '../faces';
import { ago, clock, dayLabel, loadNow, markSeen, status, stopRequest, where, type AgentCard, type ActivityItem } from './model';
import { useLive } from './use-live';
import { RequestDetails } from './request-details';

function cardState(agent: AgentCard): { label: string; tone: string; line: string } {
  if (agent.paused) return { label: 'Paused', tone: 'quiet', line: agent.paused_note ?? 'Paused' };
  if (agent.running > 0) return { label: 'Working', tone: 'working', line: `${agent.doing ?? 'Working on a request'} · today ${agent.today} requests` };
  if (agent.waiting > 0) return { label: 'Waiting', tone: 'waiting', line: agent.doing ?? 'Waiting for someone' };
  if (agent.failed_today > 0) return { label: 'Failing', tone: 'problem', line: `${agent.failed_today} could not complete today` };
  return { label: 'Idle', tone: 'quiet', line: agent.next_run ? `Next run: ${dayLabel(agent.next_run).toLowerCase()} ${clock(agent.next_run)}` : `Today ${agent.today} requests` };
}

/** Now: "while you were away", every agent's state, and work running right now
 * (designs/activity 2). The summary covers everything since the person last
 * caught up; only "See in History" or "Dismiss" moves that point. */
export function NowView({ search, openHistory }: { search: string; openHistory: (from: number) => void }) {
  const { data, error, refresh } = useLive(loadNow);
  const [open, setOpen] = useState<string>();
  const mutation = useMutation();
  const matches = (text: string) => !search || text.toLowerCase().includes(search.toLowerCase());
  const agents = (data?.agents ?? []).filter(a => matches(`${a.name} ${a.role}`));
  const running = (data?.running ?? []).filter(item => matches(`${item.summary} ${item.agent_name} ${item.conversation_name}`));
  const away = data?.away.totals;
  const stop = (item: ActivityItem) => void mutation.run(async () => { await stopRequest(item); await refresh(); mutation.setStatus('Stop requested.'); });
  const caughtUp = (then: () => Promise<void> | void) => void mutation.run(async () => { await markSeen(); await then(); });
  return <section className="activity-now" aria-label="Now">
    {error && <p role="alert">{error}</p>}
    {!data && !error && <p role="status">Loading what your agents are doing…</p>}
    {data && away && away.requests > 0 && <section className="activity-away" aria-label="While you were away">
      <span><strong>While you were away · since {dayLabel(data.away.since).toLowerCase()} {clock(data.away.since)}</strong>
        <small>{away.requests} requests · {away.done} done · {away.failed} failed · {away.waiting} waiting · {away.schedule_runs} schedule runs</small></span>
      <Button disabled={mutation.busy} onClick={() => caughtUp(() => openHistory(data.away.since))}>See in History</Button>
      <Button disabled={mutation.busy} onClick={() => caughtUp(refresh)}>Dismiss</Button>
    </section>}
    {data && <h2 className="activity-group">Agents</h2>}
    <div className="activity-cards">{agents.map(agent => {
      const state = cardState(agent);
      return <article key={agent.id} className="activity-card" aria-label={agent.name}>
        <header><AgentFace agent={{ id: agent.id, name: agent.name, avatar: agent.avatar }} className="activity-card-face" />
          <span><strong>{agent.name}</strong><small>{agent.role}</small></span>
          <span className={`activity-status ${state.tone}`}>{state.label}</span></header>
        <p>{state.line}</p>
      </article>;
    })}</div>
    {data && !agents.length && <p className="activity-empty">No agents to show.</p>}
    {data && <h2 className="activity-group">Running now · {running.length}</h2>}
    <p className="ui-status" role="status">{mutation.status}</p>
    <ul className="activity-list">{running.map(item => {
      const shown = status(item);
      return <li key={item.id}>
        <AgentFace agent={{ id: item.agent, name: item.agent_name, avatar: item.agent_avatar }} className="activity-face" />
        <span className="activity-main"><strong>{item.summary}</strong><small>{item.agent_name} · {item.requester_name ?? 'Someone'} · {where(item)} · {ago(item.created_ms)}</small></span>
        <span className={`activity-status ${shown.tone}`}>{shown.label}</span>
        {item.can_cancel ? <Button disabled={mutation.busy} onClick={() => stop(item)}>Stop</Button> : <Button onClick={() => setOpen(item.id)}>Review</Button>}
      </li>;
    })}</ul>
    {data && !running.length && <p className="activity-empty">Nothing is running right now.</p>}
    {open && <RequestDetails reply={open} close={() => setOpen(undefined)} />}
  </section>;
}
