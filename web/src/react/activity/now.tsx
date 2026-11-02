import { useCallback, useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { AgentFace } from '../faces';
import { ago, clock, dayLabel, loadNow, markSeen, pauseAgent, status, stopRequest, where, type AgentCard, type ActivityItem, type Scope } from './model';
import { useLive } from './use-live';
import { RequestDetails } from './request-details';
import { PauseAgentsControl } from './agents-pause';
import { QuestionDialog } from '../questions/dialog';
import type { AttentionItem } from '../attention/model';
import { currentWorkspace, workspaceUrl } from '../../workspace-context';

function cardState(agent: AgentCard, allPaused: boolean): { label: string; tone: string; line: string } {
  if (allPaused) return { label: 'Paused', tone: 'quiet', line: 'All agents are paused' };
  if (agent.paused) return { label: 'Paused', tone: 'quiet', line: agent.paused_note ?? 'Paused' };
  if (agent.running > 0) return { label: 'Working', tone: 'working', line: `${agent.doing ?? 'Working on a request'} · today ${agent.today} requests` };
  if (agent.waiting > 0) return { label: 'Waiting', tone: 'waiting', line: agent.doing ?? 'Waiting for someone' };
  if (agent.failed_today > 0) return { label: 'Failing', tone: 'problem', line: `${agent.failed_today} could not complete today` };
  return { label: 'Idle', tone: 'quiet', line: agent.next_run ? `Next run: ${dayLabel(agent.next_run).toLowerCase()} ${clock(agent.next_run)}` : `Today ${agent.today} requests` };
}

/** Now: "while you were away", every agent's state, and work running right now
 * (designs/activity 2). The summary covers everything since the person last
 * caught up; only "See in History" or "Dismiss" moves that point. */
export function NowView({ scope, search, openHistory, openUnhandled, waiting }: { scope: Scope; search: string; openHistory: (from: number) => void; openUnhandled?: () => void;
  /** The page's Needs me items, so a running request's question can be answered here. */
  waiting?: AttentionItem[] }) {
  const { data, error, refresh } = useLive(useCallback(() => loadNow(scope), [scope]));
  const [open, setOpen] = useState<string>();
  const [answering, setAnswering] = useState<string>();
  const mutation = useMutation();
  const matches = (text: string) => !search || text.toLowerCase().includes(search.toLowerCase());
  const agents = (data?.agents ?? []).filter(a => matches(`${a.name} ${a.role}`));
  const running = (data?.running ?? []).filter(item => matches(`${item.summary} ${item.agent_name} ${item.conversation_name}`));
  const away = data?.away.totals;
  const stop = (item: ActivityItem) => void mutation.run(async () => { await stopRequest(item); await refresh(); mutation.setStatus('Stop requested.'); });
  const caughtUp = (then: () => Promise<void> | void) => void mutation.run(async () => { await markSeen(); await then(); });
  const pause = (agent: AgentCard, paused: boolean) => void mutation.run(async () => {
    await pauseAgent(agent.id, paused); await refresh();
    mutation.setStatus(paused ? `${agent.name} is paused. It takes no new work until someone resumes it.` : `${agent.name} is working again.`);
  });
  return <section className="activity-now" aria-label="Now">
    {currentWorkspace()?.role === 'Admin' && <div className="activity-now-header"><PauseAgentsControl changed={() => void refresh().catch(() => {})} /></div>}
    {error && <p role="alert">{error}</p>}
    {!data && !error && <p role="status">Loading what your agents are doing…</p>}
    {data && away && away.requests > 0 && <section className="activity-away" aria-label="While you were away">
      <span><strong>While you were away · since {dayLabel(data.away.since).toLowerCase()} {clock(data.away.since)}</strong>
        <small>{away.requests} requests · {away.done} done · {away.failed} failed · {away.waiting} waiting · {away.schedule_runs} schedule runs</small></span>
      <Button disabled={mutation.busy} onClick={() => caughtUp(() => openHistory(data.away.since))}>See in History</Button>
      <Button disabled={mutation.busy} onClick={() => caughtUp(refresh)}>Dismiss</Button>
    </section>}
    {data && Boolean(data.unhandled_today) && <p className="activity-unhandled" role="note">
      <span>{data.unhandled_today} {data.unhandled_today === 1 ? 'event' : 'events'} not handled today</span>
      {openUnhandled && <Button onClick={openUnhandled}>See why</Button>}</p>}
    {data && <h2 className="activity-group">{scope === 'mine' ? 'My agents' : 'Agents'}</h2>}
    <div className="activity-cards">{agents.map(agent => {
      const state = cardState(agent, Boolean(data?.agents_paused));
      return <article key={agent.id} className="activity-card" aria-label={agent.name}>
        <header><AgentFace agent={{ id: agent.id, name: agent.name, avatar: agent.avatar }} className="activity-card-face" />
          <span><strong>{agent.name}</strong><small>{agent.role}</small></span>
          <span className={`activity-status ${state.tone}`}>{state.label}</span></header>
        <p>{state.line}</p>
        <div className="activity-card-actions">
          {/* Also once paused: a manager often pauses a failing agent, then fixes it. */}
          {agent.last_failed && <Button onClick={() => setOpen(agent.last_failed!)}>How to fix it</Button>}
          {agent.can_pause && !data?.agents_paused && <Button disabled={mutation.busy} aria-label={`${agent.paused ? 'Resume' : 'Pause'} ${agent.name}`}
            onClick={() => pause(agent, !agent.paused)}>{agent.paused ? 'Resume' : 'Pause'}</Button>}
          <a className="activity-card-setup" href={workspaceUrl(`/?view=agents&selected=${encodeURIComponent(agent.id)}`)}>Set up {agent.name} →</a>
        </div>
      </article>;
    })}</div>
    {data && !agents.length && <p className="activity-empty">{scope === 'mine' && !search
      ? 'You don\'t manage any agents yet. Choose All agents to see the rest.' : 'No agents to show.'}</p>}
    {data && <h2 className="activity-group">Running now · {running.length}</h2>}
    <p className="ui-status" role="status">{mutation.status}</p>
    <ul className="activity-list">{running.map(item => {
      const shown = status(item);
      return <li key={item.id}>
        <AgentFace agent={{ id: item.agent, name: item.agent_name, avatar: item.agent_avatar }} className="activity-face" />
        <span className="activity-main"><strong>{item.summary}</strong><small>{item.agent_name} · {item.requester_name ?? 'Someone'} · {where(item)} · {ago(item.created_ms)}</small></span>
        <span className={`activity-status ${shown.tone}`}>{shown.label}</span>
        {(() => {
          const question = waiting?.find(entry => entry.kind === 'question' && entry.reply === item.id);
          return question && <Button primary onClick={() => setAnswering(question.id)}>Answer</Button>;
        })()}
        {item.can_cancel ? <Button disabled={mutation.busy} onClick={() => stop(item)}>Stop</Button> : <Button onClick={() => setOpen(item.id)}>Review</Button>}
      </li>;
    })}</ul>
    {data && !running.length && <p className="activity-empty">Nothing is running right now.</p>}
    {open && <RequestDetails reply={open} close={() => setOpen(undefined)} />}
    {answering && <QuestionDialog key={answering} id={answering} close={() => { setAnswering(undefined); void refresh().catch(() => {}); }} />}
  </section>;
}
