import { useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import { AgentFace } from '../faces';
import { workspaceUrl } from '../../workspace-context';
import { changeSchedule } from '../agents/automation/model';
import { cancelFollowUp, clock, dayLabel, dismissNeed, taskUrl, type ComingItem, type FixGroup } from './model';

const fixUrl = (fix: FixGroup['fix']) => fix.kind === 'needs' ? workspaceUrl('/?view=activity&tab=needs')
  : fix.kind === 'tasks' ? workspaceUrl('/?view=activity&tab=open')
  : fix.kind === 'agent' ? workspaceUrl(`/?view=agents&selected=${encodeURIComponent(fix.ref ?? '')}`)
  : fix.kind === 'settings' ? workspaceUrl('/?view=settings&section=connections') : undefined;

/** Needs fixing (people-and-agents-first, Phase 4 AC3): what's wrong across the
 * agents you can fix, grouped by cause, each with one fix. */
export function NeedsFixing({ groups, openRequest, changed }: { groups: FixGroup[]; openRequest: (reply: string) => void; changed: () => void }) {
  const [needs, setNeeds] = useState<FixGroup>();
  if (!groups.length) return null;
  return <section className="activity-fixing" aria-label="Needs fixing">
    <h2 className="activity-group">Needs fixing · {groups.length}</h2>
    <ul className="activity-list">{groups.map(group => {
      const url = fixUrl(group.fix);
      return <li key={`${group.cause}.${group.title}`} className="activity-fix-row">
        <span className="activity-fix-faces">{group.agents.slice(0, 4).map(agent =>
          <AgentFace key={agent.id} agent={{ id: agent.id, name: agent.name }} className="activity-face" />)}</span>
        <span className="activity-main"><strong>{group.title}</strong>
          <small>{group.agents.map(agent => agent.name).join(', ')}{group.items[0]?.detail && group.cause !== 'failing' ? ` · ${group.items[0].detail}` : ''}</small></span>
        {url ? <a className="ui-button" href={url}>{group.fix.label}</a>
          : group.fix.kind === 'need' ? <Button onClick={() => setNeeds(group)}>{group.fix.label}</Button>
          : <Button onClick={() => group.fix.ref && openRequest(group.fix.ref)}>{group.fix.label}</Button>}
      </li>;
    })}</ul>
    {needs && <NeedsDialog group={needs} openRequest={openRequest} close={() => setNeeds(undefined)} changed={changed} />}
  </section>;
}

function NeedsDialog({ group, openRequest, close, changed }: { group: FixGroup; openRequest: (reply: string) => void; close: () => void; changed: () => void }) {
  const m = useMutation();
  const [gone, setGone] = useState<string[]>([]);
  return <Dialog title={group.title} close={close} busy={m.busy} status={m.status}>
    <ul className="activity-list">{group.items.filter(item => !gone.includes(item.ref)).map(item => <li key={item.ref}>
      <span className="activity-main"><strong>{item.agent_name}: {item.detail}</strong>{item.fix && <small>Fix: {item.fix}</small>}</span>
      {item.request && <Button onClick={() => { close(); openRequest(item.request!); }}>Open request</Button>}
      <Button disabled={m.busy} onClick={() => void m.run(async () => { await dismissNeed(item.ref); setGone(list => [...list, item.ref]); changed(); m.setStatus('Dismissed.'); })}>Dismiss</Button>
    </li>)}</ul>
  </Dialog>;
}

const kinds: Record<ComingItem['kind'], string> = { routine: 'Routine', follow_up: 'Follow-up', due: 'Due' };

/** Coming up (AC7): the next 24 hours of routines, follow-ups and due tasks. */
export function ComingUp({ items, changed }: { items: ComingItem[]; changed: () => void }) {
  const m = useMutation();
  if (!items.length) return null;
  const stop = (item: ComingItem) => void m.run(async () => {
    if (item.kind === 'routine') await changeSchedule(item.agent, item.id, { request: crypto.randomUUID(), revision: item.revision ?? 0, enabled: false });
    else await cancelFollowUp(item.id);
    changed();
    m.setStatus(item.kind === 'routine' ? `${item.title} is paused.` : `Follow-up for ${item.title} cancelled; the task stays open.`);
  });
  return <section className="activity-coming" aria-label="Coming up">
    <h2 className="activity-group">Coming up · next 24 hours</h2>
    <ul className="activity-list">{items.map(item => <li key={`${item.kind}.${item.id}`}>
      <time>{dayLabel(item.at) === 'Today' ? '' : `${dayLabel(item.at)} `}{clock(item.at)}</time>
      <AgentFace agent={{ id: item.agent, name: item.agent_name }} className="activity-face" />
      <span className="activity-main"><strong>{item.title}</strong><small>{kinds[item.kind]} · {item.agent_name}</small></span>
      {item.kind !== 'routine' && <a href={taskUrl(item.id)}>Open task</a>}
      {item.can_pause && item.kind !== 'due' && <Button disabled={m.busy} onClick={() => stop(item)}>{item.kind === 'routine' ? 'Pause' : 'Cancel follow-up'}</Button>}
    </li>)}</ul>
    <p className="ui-status" role="status">{m.status}</p>
  </section>;
}
