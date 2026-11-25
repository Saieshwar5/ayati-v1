import { useCallback, useMemo, useState } from 'react';
import { Button, DialogPlacement } from '../ui';
import { useMutation } from '../requests';
import { AgentFace } from '../faces';
import { ago, clock, dayLabel, loadNow, markSeen, pauseAgent, status, stopRequest, where, type AgentCard, type ActivityItem, type Scope } from './model';
import { useLive } from './use-live';
import { RequestDetails } from './request-details';
import { AgentDetails } from './agent-details';
import { fixGroups } from './fixing';
import { QuestionDialog } from '../questions/dialog';
import type { AttentionItem } from '../attention/model';
import { workspaceUrl } from '../../workspace-context';

function cardState(agent: AgentCard, allPaused: boolean): { label: string; tone: string; line: string } {
  if (allPaused) return { label: 'Paused', tone: 'quiet', line: 'All agents are paused' };
  if (agent.paused) return { label: 'Paused', tone: 'quiet', line: agent.paused_note ?? 'Paused' };
  if (agent.needs_fixing) return { label: 'Needs fixing', tone: 'problem', line: `${agent.needs_fixing} ${agent.needs_fixing === 1 ? 'thing needs' : 'things need'} fixing · in Needs you` };
  if (agent.running > 0) return { label: 'Working', tone: 'working', line: `${agent.doing ?? 'Working on a request'} · today ${agent.today} requests` };
  if (agent.waiting > 0) return { label: 'Waiting', tone: 'waiting', line: agent.doing ?? 'Waiting for someone' };
  if (agent.failed_today > 0) return { label: 'Failing', tone: 'problem', line: `${agent.failed_today} could not complete today` };
  return { label: 'Idle', tone: 'quiet', line: agent.next_run ? `Next run: ${dayLabel(agent.next_run).toLowerCase()} ${clock(agent.next_run)}` : `Today ${agent.today} requests` };
}
type Detail = { kind: 'agent' | 'request' | 'question'; id: string };

/** Work → Agents: "while you were away", every agent's state as one flat row, and
 * work running right now (designs/activity 2; rows and the side panel from
 * designs/activity-next, Design A). The summary covers everything since the person
 * last caught up; only "See in History" or "Dismiss" moves that point. With `pane`
 * (a wide window) an agent, a request or a question opens beside the list. */
export function NowView({ scope, search, openHistory, openAgentHistory, openUnhandled, openNeeds, waiting, pane = false }: {
  scope: Scope; search: string; openHistory: (from: number) => void; openAgentHistory?: (agent: string) => void; openUnhandled?: () => void;
  /** Go to Needs you, at one row when a key is given. */
  openNeeds?: (key?: string) => void;
  /** The page's Needs you items, so a running request's question can be answered here. */
  waiting?: AttentionItem[]; pane?: boolean }) {
  const { data, error, refresh } = useLive(useCallback(() => loadNow(scope), [scope]));
  const [detail, setDetail] = useState<Detail>();
  const mutation = useMutation();
  const matches = (text: string) => !search || text.toLowerCase().includes(search.toLowerCase());
  const agents = (data?.agents ?? []).filter(a => matches(`${a.name} ${a.role}`));
  const running = (data?.running ?? []).filter(item => matches(`${item.summary} ${item.agent_name} ${item.conversation_name}`));
  const away = data?.away.totals;
  const fixes = fixGroups(data?.fixing);
  const stop = (item: ActivityItem) => void mutation.run(async () => { await stopRequest(item); await refresh(); mutation.setStatus('Stop requested.'); });
  const caughtUp = (then: () => Promise<void> | void) => void mutation.run(async () => { await markSeen(); await then(); });
  const pause = (agent: AgentCard, paused: boolean) => void mutation.run(async () => {
    await pauseAgent(agent.id, paused); await refresh();
    mutation.setStatus(paused ? `${agent.name} is paused. It takes no new work until someone resumes it.` : `${agent.name} is working again.`);
  });
  const placement = useMemo(() => pane ? { settled: () => void refresh().catch(() => {}) } : undefined, [pane, refresh]);
  const close = () => { setDetail(undefined); void refresh().catch(() => {}); };
  const chosen = detail?.kind === 'agent' ? (data?.agents ?? []).find(agent => agent.id === detail.id) : undefined;
  return <section className="activity-now activity-cols activity-cols-wide" aria-label="Agents and running work">
    <div className="activity-cols-list">
      {error && <p role="alert">{error}</p>}
      {!data && !error && <p role="status">Loading what your agents are doing…</p>}
      {data && away && away.requests > 0 && <section className="activity-away" aria-label="While you were away">
        <span><strong>While you were away · since {dayLabel(data.away.since).toLowerCase()} {clock(data.away.since)}</strong>
          <small>{away.requests} requests · {away.done} done · {away.failed} failed · {away.waiting} waiting · {away.task_runs} task runs</small></span>
        <Button disabled={mutation.busy} onClick={() => caughtUp(() => openHistory(data.away.since))}>See in History</Button>
        <Button disabled={mutation.busy} onClick={() => caughtUp(refresh)}>Dismiss</Button>
      </section>}
      {data && fixes.length > 0 && <p className="activity-notice" role="note">
        <span>{fixes.length} {fixes.length === 1 ? 'thing needs' : 'things need'} fixing</span>
        {openNeeds && <Button onClick={() => openNeeds()}>Open Needs you</Button>}</p>}
      {data && Boolean(data.unhandled_today) && <p className="activity-notice" role="note">
        <span>{data.unhandled_today} {data.unhandled_today === 1 ? 'event' : 'events'} not handled today</span>
        {openUnhandled && <Button onClick={openUnhandled}>See why</Button>}</p>}
      {data && <h2 className="activity-group">{scope === 'mine' ? 'My agents' : 'Agents'}</h2>}
      <div className="activity-agents">{agents.map(agent => {
        const state = cardState(agent, Boolean(data?.agents_paused));
        return <article key={agent.id} className="activity-agent" aria-label={agent.name} aria-current={chosen?.id === agent.id ? 'true' : undefined}>
          <AgentFace agent={{ id: agent.id, name: agent.name, avatar: agent.avatar }} className="activity-card-face" />
          <span className="activity-agent-who"><button type="button" className="activity-agent-name" aria-label={`${agent.name}, details`}
            onClick={() => setDetail({ kind: 'agent', id: agent.id })}>{agent.name}</button><small>{agent.role}</small></span>
          <span className="activity-agent-state"><span className={`activity-status ${state.tone}`}>{state.label}</span><small>{state.line}</small></span>
          <div className="activity-card-actions">
            {/* Also once paused: an owner often pauses a failing agent, then fixes it. */}
            {agent.last_failed && <Button onClick={() => setDetail({ kind: 'request', id: agent.last_failed! })}>How to fix it</Button>}
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
        const question = waiting?.find(entry => entry.kind === 'question' && entry.reply === item.id);
        return <li key={item.id}>
          <AgentFace agent={{ id: item.agent, name: item.agent_name, avatar: item.agent_avatar }} className="activity-face" />
          <span className="activity-main"><strong>{item.summary}</strong><small>{item.agent_name} · {item.requester_name ?? 'Someone'} · {where(item)} · {ago(item.created_ms)}</small></span>
          <span className={`activity-status ${shown.tone}`}>{shown.label}</span>
          {question && <Button primary onClick={() => setDetail({ kind: 'question', id: question.id })}>Answer</Button>}
          {item.can_cancel ? <Button disabled={mutation.busy} onClick={() => stop(item)}>Stop</Button> : <Button onClick={() => setDetail({ kind: 'request', id: item.id })}>Review</Button>}
        </li>;
      })}</ul>
      {data && !running.length && <p className="activity-empty">Nothing is running right now.</p>}
      {data && <p className="ui-help activity-next">What agents will do on their own, and when, is on <a href={workspaceUrl('/?view=activity&tab=planned')}>Planned</a>.</p>}
    </div>
    <DialogPlacement.Provider value={placement}><div className="activity-pane">
      {detail?.kind === 'request' && <RequestDetails key={detail.id} reply={detail.id} close={close} />}
      {detail?.kind === 'question' && <QuestionDialog key={detail.id} id={detail.id} close={close} />}
      {chosen && <AgentDetails key={chosen.id} agent={chosen} state={cardState(chosen, Boolean(data?.agents_paused))} busy={mutation.busy} close={close}
        needs={(waiting ?? []).filter(item => item.agent === chosen.id)} fixes={fixes.filter(group => group.agents.some(agent => agent.id === chosen.id))}
        running={(data?.running ?? []).filter(item => item.agent === chosen.id)} coming={(data?.coming_up ?? []).filter(item => item.agent === chosen.id)}
        openNeeds={key => openNeeds?.(key)} openRequest={id => setDetail({ kind: 'request', id })} openHistory={() => openAgentHistory?.(chosen.id)} stop={stop} />}
      {pane && data && !detail && agents.length > 0 && <p className="activity-empty">Choose an agent’s name to see what waits on you, what it is doing and what comes next.</p>}
    </div></DialogPlacement.Provider>
  </section>;
}
