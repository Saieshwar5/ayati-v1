import { Button, Dialog } from '../ui';
import { workspaceUrl } from '../../workspace-context';
import { attentionKey, type AttentionItem } from '../attention/model';
import { fixKey } from './fixing';
import { ago, clock, dayLabel, status, where, type ActivityItem, type AgentCard, type ComingItem, type FixGroup } from './model';

const comingLabel: Record<ComingItem['kind'], string> = { follow_up: 'Next run', due: 'Task due' };
const clean = (title: string) => title.replace(/^\[Ayati request mode:[^\]]*\]\s*/, '');

/** One agent on Work, beside the list (designs/activity-next, Design A): what waits on
 * you, what it is doing, what comes next and how today went, from the same reads as
 * the rest of the tab. Its buttons are the ones the row and the lists already have. */
export function AgentDetails({ agent, state, needs, fixes, running, coming, busy, close, openNeeds, openRequest, openHistory, stop }: {
  agent: AgentCard; state: { label: string; tone: string; line: string }; needs: AttentionItem[]; fixes: FixGroup[];
  running: ActivityItem[]; coming: ComingItem[]; busy: boolean; close: () => void;
  openNeeds: (key: string) => void; openRequest: (reply: string) => void; openHistory: () => void; stop: (item: ActivityItem) => void;
}) {
  return <Dialog drawer title={agent.name} close={close} busy={busy}>
    <section className="activity-details" aria-label={`${agent.name} today`}>
      <span className={`activity-status ${state.tone}`}>{state.label}</span>
      <p className="activity-task-line">{agent.role} · {state.line}</p>
      {needs.length + fixes.length > 0 && <><h3>Needs you · {needs.length + fixes.length}</h3>
        <ul className="activity-list">
          {needs.map(item => <li key={attentionKey(item)}><span className="activity-main"><strong>{clean(item.title)}</strong><small>{item.conversation_name} · {ago(item.created_at ?? item.at)}</small></span>
            <Button onClick={() => openNeeds(attentionKey(item))}>Open in Needs you</Button></li>)}
          {fixes.map(group => <li key={fixKey(group)}><span className="activity-main"><strong>{group.title}</strong><small>Needs fixing</small></span>
            <Button onClick={() => openNeeds(fixKey(group))}>Open in Needs you</Button></li>)}
        </ul></>}
      <h3>Doing now · {running.length}</h3>
      {running.length ? <ul className="activity-list">{running.map(item => <li key={item.id}>
        <span className="activity-main"><strong>{item.summary}</strong><small>{item.requester_name ?? 'Someone'} · {where(item)} · {status(item).label.toLowerCase()} · {ago(item.created_ms)}</small></span>
        {item.can_cancel ? <Button disabled={busy} onClick={() => stop(item)}>Stop</Button> : <Button onClick={() => openRequest(item.id)}>Review</Button>}
      </li>)}</ul> : <p className="ui-help">Nothing is running right now.</p>}
      {coming.length > 0 && <><h3>Coming up</h3>
        <ul className="activity-list">{coming.map(item => <li key={`${item.kind}.${item.id}`}>
          <time>{dayLabel(item.at) === 'Today' ? '' : `${dayLabel(item.at)} `}{clock(item.at)}</time>
          <span className="activity-main"><strong>{item.title}</strong><small>{comingLabel[item.kind]}</small></span></li>)}</ul></>}
      <h3>Today</h3>
      <p className="activity-task-line">{agent.today} {agent.today === 1 ? 'request' : 'requests'}{agent.failed_today ? ` · ${agent.failed_today} could not complete` : ''}
        {agent.next_run ? ` · next run ${dayLabel(agent.next_run).toLowerCase()} ${clock(agent.next_run)}` : ''}</p>
      <div className="activity-detail-actions">
        {agent.last_failed && <Button onClick={() => openRequest(agent.last_failed!)}>How to fix it</Button>}
        <Button onClick={openHistory}>All history</Button>
        <a className="ui-button" href={workspaceUrl(`/?view=agents&selected=${encodeURIComponent(agent.id)}`)}>Set up {agent.name} →</a>
      </div>
    </section>
  </Dialog>;
}
