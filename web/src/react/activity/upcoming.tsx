import { useCallback, useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { AgentFace } from '../faces';
import { workspaceUrl } from '../../workspace-context';
import { agentSectionUrl } from '../agents/workspace-tabs';
import { cadence } from '../agents/automation/model';
import { clock, taskUrl, type Scope } from './model';
import { useLive } from './use-live';
import { cancel, filters, grouped, loadUpcoming, setRunning, type Filter, type UpcomingItem, type UpcomingSchedule } from './upcoming-model';

const kindLabel: Record<UpcomingItem['kind'], string> = { schedule: 'Schedule', trigger: 'Trigger', app_trigger: 'Trigger', follow_up: 'Follow-up', due: 'Due' };
type Undo = { label: string; item: UpcomingSchedule; revision: number };

/** Activity › Upcoming (designs/automations-simplified 3): everything agents will do
 * on their own, with Pause and Resume in place and a link to where each is set up. */
export function UpcomingView({ scope, search }: { scope: Scope; search: string }) {
  const { data, error, refresh } = useLive(useCallback(() => loadUpcoming(scope), [scope]));
  const [filter, setFilter] = useState<Filter>('all');
  const [undo, setUndo] = useState<Undo>();
  const m = useMutation();
  const act = (work: () => Promise<Undo | undefined>, done: string) => void m.run(async () => {
    setUndo(undefined);
    const next = await work();
    await refresh();
    setUndo(next);
    m.setStatus(done);
  });
  if (error) return <p role="alert">{error}</p>;
  if (!data) return <p role="status">Loading what’s coming up…</p>;
  const groups = grouped(data.items, filter, search);
  return <section className="activity-upcoming" aria-label="Upcoming">
    <div className="activity-kinds" role="group" aria-label="Kind">{filters.map(([value, label]) =>
      <Button key={value} aria-pressed={filter === value} onClick={() => setFilter(value)}>{label}</Button>)}</div>
    <p className="ui-status" role="status">{m.busy ? 'Saving…' : m.status}
      {undo && !m.busy && <> <Button className="activity-undo" onClick={() => act(async () => { await setRunning(undo.item, true, undo.revision); return undefined; }, `${undo.label} is back on.`)}>Undo</Button></>}</p>
    {groups.map(([day, items]) => <section key={day} aria-label={day}>
      <h2 className="activity-group">{day}</h2>
      <ul className="activity-list">{items.map(item => <li key={`${item.kind}.${item.id}.${'app' in item ? item.app : ''}`}>
        <time>{'at' in item && item.at && day !== 'Paused' ? clock(item.at) : '—'}</time>
        <AgentFace agent={{ id: item.agent, name: item.agent_name }} className="activity-face" />
        <span className="activity-main"><strong>{item.title}</strong>
          <small>{kindLabel[item.kind]} · {item.agent_name} · {about(item)}</small></span>
        <Actions item={item} busy={m.busy} act={act} />
      </li>)}</ul>
    </section>)}
    {!groups.length && <p className="activity-empty">{data.items.length ? 'Nothing matches.' : 'Nothing runs on its own yet. Add schedules and triggers on an agent’s Automations tab, or ask an agent in Chat.'}</p>}
  </section>;
}

function about(item: UpcomingItem): string {
  if (item.kind === 'schedule') return [item.state === 'blocked' ? 'Blocked' : item.state === 'paused' ? 'Paused' : cadence(item.interval_seconds), `set up by ${item.own ? 'you' : item.by}`].join(' · ');
  if (item.kind === 'trigger') return [item.state === 'enabled' ? item.source ?? 'When something happens' : item.state === 'paused' ? 'Paused' : 'Blocked', `set up by ${item.own ? 'you' : item.by}`].join(' · ');
  if (item.kind === 'app_trigger') return `Added from ${item.app_name}`;
  return item.kind === 'follow_up' ? 'The agent checks back on this task' : 'This task is due';
}

function Actions({ item, busy, act }: { item: UpcomingItem; busy: boolean; act: (work: () => Promise<Undo | undefined>, done: string) => void }) {
  const edit = <a href={agentSectionUrl(item.agent, 'Automations')}>Edit in {item.agent_name}</a>;
  if (item.kind === 'follow_up') return <span className="activity-actions">
    <Button disabled={busy} onClick={() => act(async () => { await cancel(item); return undefined; }, `Follow-up for ${item.title} cancelled; the task stays open.`)}>Cancel follow-up</Button>
    <a href={taskUrl(item.id)}>Open task</a></span>;
  if (item.kind === 'due') return <span className="activity-actions"><a href={taskUrl(item.id)}>Open task</a></span>;
  if (item.kind === 'app_trigger') return <span className="activity-actions"><a href={workspaceUrl('/?view=apps')}>{item.manages ? 'Change' : 'Open'} in Apps</a></span>;
  if (item.state === 'enabled') return <span className="activity-actions">
    <Button disabled={busy} onClick={() => act(async () => {
      const result = await setRunning(item, false);
      return item.kind === 'schedule' && item.own ? { label: item.title, item, revision: result.revision } : undefined;
    }, item.kind === 'trigger' ? `${item.title} is paused. Resume it on ${item.agent_name}’s Automations tab, after checking its access.`
      : item.own ? `${item.title} is paused.` : `${item.title} is paused. Only ${item.by} can resume it.`)}>Pause</Button>
    {edit}</span>;
  return <span className="activity-actions">
    {item.kind === 'schedule' && item.own && <Button disabled={busy} onClick={() => act(async () => { await setRunning(item, true); return undefined; }, `${item.title} is back on.`)}>Resume</Button>}
    {item.kind === 'schedule' && !item.own && <small>Only {item.by} can resume it.</small>}
    {item.kind === 'trigger' ? <a href={agentSectionUrl(item.agent, 'Automations')}>Review and resume in {item.agent_name}</a> : edit}</span>;
}
