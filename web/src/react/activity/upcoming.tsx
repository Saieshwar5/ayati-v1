import { useCallback, useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { AgentFace } from '../faces';
import { agentSectionUrl } from '../agents/workspace-tabs';
import { clock, taskUrl, type Scope } from './model';
import { useLive } from './use-live';
import { cancel, filters, grouped, loadUpcoming, pauseTrigger, type Filter, type UpcomingItem } from './upcoming-model';

const kindLabel: Record<UpcomingItem['kind'], string> = { trigger: 'Trigger', follow_up: 'Follow-up', due: 'Due' };

/** Activity › Calendar › List (designs/automations-simplified 3, Planned until Design A):
 * what agents will do on their own, from now on: follow-ups and booked task runs,
 * due tasks and triggers, with a link to where each is looked after. */
export function UpcomingView({ scope, search }: { scope: Scope; search: string }) {
  const { data, error, refresh } = useLive(useCallback(() => loadUpcoming(scope), [scope]));
  const [filter, setFilter] = useState<Filter>('all');
  const m = useMutation();
  const act = (work: () => Promise<unknown>, done: string) => void m.run(async () => {
    await work();
    await refresh();
    m.setStatus(done);
  });
  if (error) return <p role="alert">{error}</p>;
  if (!data) return <p role="status">Loading what’s coming up…</p>;
  const groups = grouped(data.items, filter, search);
  return <section className="activity-upcoming" aria-label="Planned">
    <div className="activity-kinds" role="group" aria-label="Kind">{filters.map(([value, label]) =>
      <Button key={value} aria-pressed={filter === value} onClick={() => setFilter(value)}>{label}</Button>)}</div>
    <p className="ui-status" role="status">{m.busy ? 'Saving…' : m.status}</p>
    {groups.length > 0 && <p className="activity-nowline" aria-hidden="true">Now · {clock(Date.now())}</p>}
    {groups.map(([day, items]) => <section key={day} aria-label={day}>
      <h2 className="activity-group">{day}</h2>
      <ul className="activity-list">{items.map(item => <li key={`${item.kind}.${item.id}.${'at' in item ? item.at : ''}`}>
        <time>{'at' in item ? clock(item.at) : '—'}</time>
        <AgentFace agent={{ id: item.agent, name: item.agent_name }} className="activity-face" />
        <span className="activity-main"><strong>{item.title}</strong>
          <small>{kindLabel[item.kind]} · {item.agent_name} · {about(item)}</small></span>
        <Actions item={item} busy={m.busy} act={act} />
      </li>)}</ul>
    </section>)}
    {!groups.length && <p className="activity-empty">{data.items.length ? 'Nothing matches.' : 'Nothing runs on its own yet. Ask an agent in Chat, for example “every morning at 9, tell me which orders are late”.'}</p>}
  </section>;
}

function about(item: UpcomingItem): string {
  if (item.kind === 'trigger') return [item.state === 'enabled' ? item.source ?? 'When something happens' : item.state === 'paused' ? 'Paused' : 'Blocked', `set up by ${item.own ? 'you' : item.by}`].join(' · ');
  return item.kind === 'follow_up' ? 'The agent checks back on this task' : 'This task is due';
}

function Actions({ item, busy, act }: { item: UpcomingItem; busy: boolean; act: (work: () => Promise<unknown>, done: string) => void }) {
  if (item.kind === 'follow_up') return <span className="activity-actions">
    <Button disabled={busy} onClick={() => act(() => cancel(item), `Follow-up for ${item.title} cancelled; the task stays open.`)}>Cancel follow-up</Button>
    <a href={taskUrl(item.id)}>Open task</a></span>;
  if (item.kind === 'due') return <span className="activity-actions"><a href={taskUrl(item.id)}>Open task</a></span>;
  if (item.state === 'enabled') return <span className="activity-actions">
    <Button disabled={busy} onClick={() => act(() => pauseTrigger(item), `${item.title} is paused. Resume it on ${item.agent_name}’s Automations tab, after checking its access.`)}>Pause</Button>
    <a href={agentSectionUrl(item.agent, 'Automations')}>Edit in {item.agent_name}</a></span>;
  return <span className="activity-actions"><a href={agentSectionUrl(item.agent, 'Automations')}>Review and resume in {item.agent_name}</a></span>;
}
