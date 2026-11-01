import { useCallback, useState } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { AgentFace } from '../faces';
import { clock, dayLabel, loadUnhandled, type Scope, type Unhandled } from './model';

/** Events that arrived but no agent acted on (paused, blocked, access changed), with
 * the recorded reason. They never became requests, so they have their own list:
 * History → Type → Events not handled, or the line on Now. */
export function UnhandledEvents({ agent, from, scope }: { agent?: string; from?: number; scope?: Scope }) {
  const first = useResource(useCallback(() => loadUnhandled({ agent, from, scope }), [agent, from, scope]));
  const [older, setOlder] = useState<{ items: Unhandled[]; next: number | null }>();
  const more = useMutation();
  const items = [...(first.data?.items ?? []), ...(older?.items ?? [])];
  const next = older ? older.next : first.data?.next_before ?? null;
  const loadOlder = (before: number) => void more.run(async () => {
    const page = await loadUnhandled({ agent, from, scope }, before);
    setOlder(old => ({ items: [...(old?.items ?? []), ...page.items], next: page.next_before }));
  });
  let day = '';
  return <div className="activity-timeline" aria-label="Events not handled">
    {first.error && <p role="alert">{first.error}</p>}
    {!first.data && !first.error && <p role="status">Loading events…</p>}
    {first.data && !items.length && <p className="activity-empty">Every event in this period was handled.</p>}
    <ul className="activity-list">{items.map(event => {
      const label = dayLabel(event.at); const heading = label !== day; day = label;
      return <li key={event.id} className="activity-row">
        {heading && <h3 className="activity-day">{label}</h3>}
        <div className="activity-row-button">
          <time>{clock(event.at)}</time>
          <span className="activity-agent"><AgentFace agent={{ id: event.agent, name: event.agent_name, avatar: event.agent_avatar }} className="activity-face" />{event.agent_name}</span>
          <span className="activity-main"><strong>{event.title}</strong><small>{event.reason ?? 'Not handled.'}</small></span>
          <span className="activity-status problem">Not handled</span>
        </div>
      </li>;
    })}</ul>
    {next && <Button disabled={more.busy} onClick={() => loadOlder(next)}>{more.busy ? 'Loading…' : 'Show older'}</Button>}
    {more.status && <p role="alert">{more.status}</p>}
  </div>;
}
